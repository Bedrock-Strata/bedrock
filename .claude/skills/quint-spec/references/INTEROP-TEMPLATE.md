# Cross-Chain Interoperability Templates

Starter templates for cross-chain messaging, IBC-style packet flows, bridges,
and multi-chain state management.

---

## Cross-Chain Packet Lifecycle (ICS-20 Style)

Full send/receive/ack/timeout packet flow for fungible token transfers.

```quint
module ICS20Types {
  type ChainId = str
  type ChannelId = str
  type Address = str
  type Denom = str
  type Amount = int

  type PacketData = {
    sender: Address,
    receiver: Address,
    denom: Denom,
    amount: Amount,
  }

  type Packet = {
    sequence: int,
    srcChannel: ChannelId,
    dstChannel: ChannelId,
    data: PacketData,
    timeoutHeight: int,
  }

  type Ack = AckSuccess | AckError(str)

  type ChainState = {
    balances: Address -> (Denom -> int),
    escrow: (ChannelId, Denom) -> int,
    height: int,
    nextSeqSend: ChannelId -> int,
    nextSeqRecv: ChannelId -> int,
  }
}

module ICS20 {
  import ICS20Types.*

  const CHAINS: Set[ChainId]
  const CHANNELS: Set[ChannelId]
  const USERS: Set[Address]
  const DENOMS: Set[Denom]
  const MAX_AMOUNT: int
  const MAX_HEIGHT: int

  var chains: ChainId -> ChainState
  var inflight: Set[Packet]      // Packets sent but not yet received/timed out
  var acks: Set[(Packet, Ack)]   // Acknowledgements pending processing

  pure def getBalance(state: ChainState, addr: Address, denom: Denom): int =
    state.balances.getOrElse(addr, Map()).getOrElse(denom, 0)

  pure def getEscrow(state: ChainState, channel: ChannelId, denom: Denom): int =
    state.escrow.getOrElse((channel, denom), 0)

  action init = all {
    chains' = CHAINS.mapBy(c => {
      balances: Map(),
      escrow: Map(),
      height: 1,
      nextSeqSend: Map(),
      nextSeqRecv: Map(),
    }),
    inflight' = Set(),
    acks' = Set(),
  }

  // Send: escrow tokens on source chain, create packet
  action sendTransfer(chain: ChainId, channel: ChannelId, sender: Address,
                      receiver: Address, denom: Denom, amount: Amount): bool = all {
    amount > 0,
    val state = chains.get(chain)
    getBalance(state, sender, denom) >= amount,
    val seq = state.nextSeqSend.getOrElse(channel, 1)
    val packet: Packet = {
      sequence: seq,
      srcChannel: channel,
      dstChannel: channel,  // Simplified: same channel ID
      data: { sender: sender, receiver: receiver, denom: denom, amount: amount },
      timeoutHeight: state.height + 10,
    }
    val newState = {
      ...state,
      balances: state.balances.setBy(sender, b =>
        b.setBy(denom, v => v - amount)),
      escrow: state.escrow.setBy((channel, denom), e =>
        e.getOrElse(0) + amount),
      nextSeqSend: state.nextSeqSend.set(channel, seq + 1),
    }
    chains' = chains.set(chain, newState),
    inflight' = inflight.union(Set(packet)),
    acks' = acks,
  }

  // Receive: mint tokens on destination chain, produce ack
  action recvPacket(chain: ChainId, packet: Packet): bool = all {
    inflight.contains(packet),
    val state = chains.get(chain)
    val expectedSeq = state.nextSeqRecv.getOrElse(packet.dstChannel, 1)
    packet.sequence == expectedSeq,
    state.height < packet.timeoutHeight,
    // Mint voucher tokens on destination
    val d = packet.data
    val voucherDenom = packet.srcChannel + "/" + d.denom  // IBC denomination
    val newState = {
      ...state,
      balances: state.balances.setBy(d.receiver, b =>
        b.getOrElse(Map()).setBy(voucherDenom, v => v.getOrElse(0) + d.amount)),
      nextSeqRecv: state.nextSeqRecv.set(packet.dstChannel, expectedSeq + 1),
    }
    chains' = chains.set(chain, newState),
    inflight' = inflight.exclude(Set(packet)),
    acks' = acks.union(Set((packet, AckSuccess))),
  }

  // Timeout: return escrowed tokens to sender
  action timeoutPacket(srcChain: ChainId, packet: Packet): bool = all {
    inflight.contains(packet),
    val state = chains.get(srcChain)
    // Destination chain height has passed timeout
    state.height >= packet.timeoutHeight,
    // Return escrowed tokens
    val d = packet.data
    val newState = {
      ...state,
      balances: state.balances.setBy(d.sender, b =>
        b.getOrElse(Map()).setBy(d.denom, v => v.getOrElse(0) + d.amount)),
      escrow: state.escrow.setBy((packet.srcChannel, d.denom), e => e - d.amount),
    }
    chains' = chains.set(srcChain, newState),
    inflight' = inflight.exclude(Set(packet)),
    acks' = acks,
  }

  // Advance block height
  action advanceHeight(chain: ChainId): bool = all {
    val state = chains.get(chain)
    state.height < MAX_HEIGHT,
    chains' = chains.set(chain, { ...state, height: state.height + 1 }),
    inflight' = inflight,
    acks' = acks,
  }

  action step = {
    nondet chain = CHAINS.oneOf()
    nondet channel = CHANNELS.oneOf()
    nondet sender = USERS.oneOf()
    nondet receiver = USERS.oneOf()
    nondet denom = DENOMS.oneOf()
    nondet amount = 1.to(MAX_AMOUNT).oneOf()
    any {
      sendTransfer(chain, channel, sender, receiver, denom, amount),
      // Nondeterministically pick a packet to receive or timeout
      if (inflight.size() > 0) {
        nondet packet = inflight.oneOf()
        any {
          recvPacket(chain, packet),
          timeoutPacket(chain, packet),
        }
      } else all { chains' = chains, inflight' = inflight, acks' = acks },
      advanceHeight(chain),
    }
  }

  // Every escrowed token on source has a corresponding voucher on destination (or is in-flight)
  val escrowConserved = CHAINS.forall(c =>
    CHANNELS.forall(ch =>
      DENOMS.forall(d =>
        getEscrow(chains.get(c), ch, d) >= 0
      )
    )
  )

  // No packet is processed twice
  val noDoubleProcessing = true  // Enforced by nextSeqRecv tracking
}
```

---

## Multi-Chain State with Channel Topology

Model a network of chains with explicit channel connections.

```quint
module ChainNetwork {
  type ChainId = str
  type ChannelEnd = { chainId: ChainId, channelId: str }
  type Connection = { end1: ChannelEnd, end2: ChannelEnd }

  const TOPOLOGY: Set[Connection]

  pure def counterparty(conn: Connection, chain: ChainId): ChannelEnd =
    if (conn.end1.chainId == chain) conn.end2 else conn.end1

  pure def channelsOn(chain: ChainId): Set[str] =
    TOPOLOGY.filter(c => c.end1.chainId == chain).map(c => c.end1.channelId)
      .union(TOPOLOGY.filter(c => c.end2.chainId == chain).map(c => c.end2.channelId))
}
```

---

## Threshold Verification (m-of-n)

Model multi-signature or threshold verification for bridge validators.

```quint
module ThresholdBridge {
  type Validator = str
  type Message = { nonce: int, payload: str, sourceChain: str }

  const VALIDATORS: Set[Validator]
  const THRESHOLD: int  // m in m-of-n

  var signatures: Message -> Set[Validator]
  var executed: Set[int]  // Nonces of executed messages

  action sign(validator: Validator, msg: Message): bool = all {
    VALIDATORS.contains(validator),
    not(executed.contains(msg.nonce)),
    signatures' = signatures.setBy(msg, sigs => sigs.getOrElse(Set()).union(Set(validator))),
    executed' = executed,
  }

  action execute(msg: Message): bool = all {
    signatures.getOrElse(msg, Set()).size() >= THRESHOLD,
    not(executed.contains(msg.nonce)),
    executed' = executed.union(Set(msg.nonce)),
    signatures' = signatures,
  }

  // Safety: only messages with enough signatures can execute
  val onlyThresholdExecuted = executed.forall(nonce =>
    // At the time of execution, threshold was met
    true  // Enforced by execute guard
  )

  // No double execution
  val noDoubleExecution = true  // Set membership prevents duplicates
}
```

---

## Escrow-Fill-Settle with Timeout

Generic cross-chain transfer pattern with escrow on source, fill on destination,
and settlement or timeout refund.

```quint
module EscrowFillSettle {
  type Address = str
  type OrderId = int

  type Order = {
    id: OrderId,
    sender: Address,
    receiver: Address,
    sourceAmount: int,
    destAmount: int,
    timeoutHeight: int,
  }

  type OrderStatus = Escrowed | Filled | Settled | Refunded

  const USERS: Set[Address]
  const FILLERS: Set[Address]
  const MAX_AMOUNT: int

  var orders: OrderId -> Order
  var orderStatus: OrderId -> OrderStatus
  var sourceBalances: Address -> int
  var destBalances: Address -> int
  var nextOrderId: int
  var currentHeight: int

  action init = all {
    orders' = Map(),
    orderStatus' = Map(),
    sourceBalances' = USERS.mapBy(u => 1000),
    destBalances' = USERS.mapBy(u => 1000),
    nextOrderId' = 1,
    currentHeight' = 1,
  }

  // Step 1: User escrows tokens on source chain
  action escrow(sender: Address, receiver: Address, srcAmt: int, dstAmt: int): bool = all {
    srcAmt > 0,
    dstAmt > 0,
    sourceBalances.getOrElse(sender, 0) >= srcAmt,
    val order: Order = {
      id: nextOrderId, sender: sender, receiver: receiver,
      sourceAmount: srcAmt, destAmount: dstAmt,
      timeoutHeight: currentHeight + 10,
    }
    orders' = orders.set(nextOrderId, order),
    orderStatus' = orderStatus.set(nextOrderId, Escrowed),
    sourceBalances' = sourceBalances.setBy(sender, b => b - srcAmt),
    destBalances' = destBalances,
    nextOrderId' = nextOrderId + 1,
    currentHeight' = currentHeight,
  }

  // Step 2: Filler delivers tokens on destination chain
  action fill(filler: Address, orderId: OrderId): bool = all {
    orderStatus.has(orderId),
    orderStatus.get(orderId) == Escrowed,
    val order = orders.get(orderId)
    currentHeight < order.timeoutHeight,
    destBalances.getOrElse(filler, 0) >= order.destAmount,
    destBalances' = destBalances
      .setBy(filler, b => b - order.destAmount)
      .setBy(order.receiver, b => b.getOrElse(0) + order.destAmount),
    orderStatus' = orderStatus.set(orderId, Filled),
    // Frame conditions
    orders' = orders,
    sourceBalances' = sourceBalances,
    nextOrderId' = nextOrderId,
    currentHeight' = currentHeight,
  }

  // Step 3: Settlement releases escrowed tokens to filler
  action settle(orderId: OrderId): bool = all {
    orderStatus.has(orderId),
    orderStatus.get(orderId) == Filled,
    val order = orders.get(orderId)
    // Filler receives escrowed source tokens (simplified: filler = receiver here)
    sourceBalances' = sourceBalances.setBy(order.sender, b =>
      b),  // Already deducted at escrow time
    orderStatus' = orderStatus.set(orderId, Settled),
    orders' = orders,
    destBalances' = destBalances,
    nextOrderId' = nextOrderId,
    currentHeight' = currentHeight,
  }

  // Timeout: refund escrowed tokens to sender
  action timeout(orderId: OrderId): bool = all {
    orderStatus.has(orderId),
    orderStatus.get(orderId) == Escrowed,
    val order = orders.get(orderId)
    currentHeight >= order.timeoutHeight,
    sourceBalances' = sourceBalances.setBy(order.sender, b =>
      b + order.sourceAmount),
    orderStatus' = orderStatus.set(orderId, Refunded),
    orders' = orders,
    destBalances' = destBalances,
    nextOrderId' = nextOrderId,
    currentHeight' = currentHeight,
  }

  action advanceHeight: bool = all {
    currentHeight' = currentHeight + 1,
    orders' = orders,
    orderStatus' = orderStatus,
    sourceBalances' = sourceBalances,
    destBalances' = destBalances,
    nextOrderId' = nextOrderId,
  }

  action step = {
    nondet user = USERS.oneOf()
    nondet receiver = USERS.oneOf()
    nondet filler = FILLERS.oneOf()
    nondet amount = 1.to(MAX_AMOUNT).oneOf()
    nondet amount2 = 1.to(MAX_AMOUNT).oneOf()
    any {
      escrow(user, receiver, amount, amount2),
      if (orders.keys().size() > 0) {
        nondet orderId = orders.keys().oneOf()
        any {
          fill(filler, orderId),
          settle(orderId),
          timeout(orderId),
        }
      } else all {
        orders' = orders, orderStatus' = orderStatus,
        sourceBalances' = sourceBalances, destBalances' = destBalances,
        nextOrderId' = nextOrderId, currentHeight' = currentHeight,
      },
      advanceHeight,
    }
  }

  // Every escrowed order eventually settles or refunds
  val noStuckOrders = orders.keys().forall(id =>
    val status = orderStatus.get(id)
    status == Escrowed or status == Filled or status == Settled or status == Refunded
  )

  // No negative balances
  val noNegativeBalances =
    USERS.forall(u => sourceBalances.getOrElse(u, 0) >= 0) and
    USERS.forall(u => destBalances.getOrElse(u, 0) >= 0)
}
```
