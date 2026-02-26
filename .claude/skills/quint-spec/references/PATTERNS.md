# Proven Quint Patterns

Patterns extracted from production specifications including Cosmos Bank, ICS-20,
CCV (Cross-Chain Validation), Neutron DEX, Timewave Vault, and ZKSync Governance.

---

## 1. Parameterized Module + Concrete Instance

Separate protocol logic (parameterized) from test configuration (concrete values).
This enables reuse and makes verification tractable by controlling state space size.

```quint
module TokenTransfer {
  const CHAINS: Set[str]
  const DENOMS: Set[str]
  const MAX_AMOUNT: int

  var balances: str -> (str -> int)
  // ... protocol logic using constants
}

module TokenTransferTest {
  import TokenTransfer(
    CHAINS = Set("hub", "osmosis"),
    DENOMS = Set("uatom"),
    MAX_AMOUNT = 100,
  ).*
  // ... tests and verification
}
```

**When to use:** Always. Every spec should separate parameters from logic.

---

## 2. Message-Passing via Set Accumulation

Model asynchronous communication by accumulating messages into a set, then
nondeterministically selecting and processing them.

```quint
var pendingMessages: Set[Msg]

action sendMsg(msg: Msg): bool = all {
  pendingMessages' = pendingMessages.union(Set(msg)),
  // ... other frame conditions
}

action processOneMessage: bool = {
  nondet msg = pendingMessages.oneOf()
  all {
    handleMessage(msg),
    pendingMessages' = pendingMessages.exclude(Set(msg)),
  }
}
```

**When to use:** Cross-chain messaging, event-driven systems, actor models.

---

## 3. Guard + Update (Precondition Pattern)

Every action consists of guards (boolean preconditions without primes) followed by
updates (assignments to primed variables). Guards and updates compose with `all`.

```quint
action withdraw(user: str, amount: int): bool = all {
  // Guards (no primes)
  amount > 0,
  balanceOf(user) >= amount,
  not(paused),
  // Updates (primes)
  balances' = balances.setBy(user, b => b - amount),
  totalLocked' = totalLocked - amount,
  paused' = paused,
}
```

**When to use:** Every action. This is the fundamental action pattern in Quint.

---

## 4. Nondeterministic Step with Process Selection

The top-level `step` action selects a participant and action nondeterministically.
The model checker explores all combinations.

```quint
val USERS = Set("alice", "bob", "carol")
val AMOUNTS = 1.to(50)

action step = {
  nondet user = USERS.oneOf()
  nondet amount = AMOUNTS.oneOf()
  nondet target = USERS.oneOf()
  any {
    deposit(user, amount),
    withdraw(user, amount),
    transfer(user, target, amount),
  }
}
```

**When to use:** Always in the main spec module. This defines the state exploration space.

---

## 5. Sum Types for Protocol Messages and States

Use sum types (variants) to model different message kinds and protocol states.
Pattern match to handle each case.

```quint
type PacketData =
  | TransferData({ sender: str, receiver: str, denom: str, amount: int })
  | IcaExecute({ controller: str, msgs: List[str] })

type ChannelState = Init | TryOpen | Open | Closed

action handlePacket(data: PacketData): bool =
  match data {
    | TransferData(t) => processTransfer(t.sender, t.receiver, t.denom, t.amount)
    | IcaExecute(i) => processIca(i.controller, i.msgs)
  }
```

**When to use:** Whenever a protocol has multiple message types or state phases.

---

## 6. Result/Option Types for Error Handling

Model success and failure explicitly. Actions return `bool` but use Result types
internally for complex logic.

```quint
type Result = Ok(int) | Err(str)

pure def safeDiv(a: int, b: int): Result =
  if (b == 0) Err("division by zero")
  else Ok(a / b)

pure def computeShares(assets: int, totalAssets: int, totalShares: int): Result =
  if (totalAssets == 0) Ok(assets)  // First depositor
  else safeDiv(assets * totalShares, totalAssets)
```

**When to use:** Complex computations where multiple things can go wrong.

---

## 7. Ghost Variables for Verification

Variables that track properties but don't affect protocol behavior. Useful for
counting events, tracking history, or maintaining running totals for invariants.

```quint
// Ghost: not read by any action's guards
var ghostTotalDeposited: int
var ghostTotalWithdrawn: int
var ghostActionLog: List[str]

action deposit(user: str, amount: int): bool = all {
  // Real logic
  balances' = balances.setBy(user, b => b + amount),
  // Ghost updates
  ghostTotalDeposited' = ghostTotalDeposited + amount,
  ghostActionLog' = ghostActionLog.append("deposit"),
  // Other frame conditions
  ghostTotalWithdrawn' = ghostTotalWithdrawn,
}

// Now we can write invariants over the full history
val flowConservation =
  ghostTotalDeposited - ghostTotalWithdrawn == currentTotalBalance
```

**When to use:** When invariants need historical information (totals, counts, sequences).

---

## 8. Record Spread for State Updates

Use spread syntax to update specific fields while keeping others unchanged.
Especially useful with complex nested state.

```quint
type ChainState = {
  balances: str -> int,
  supply: int,
  height: int,
  pendingPackets: Set[Packet],
}

pure def updateBalance(state: ChainState, addr: str, delta: int): ChainState =
  { ...state, balances: state.balances.setBy(addr, b => b + delta) }

pure def advanceHeight(state: ChainState): ChainState =
  { ...state, height: state.height + 1 }
```

**When to use:** When state is a record with many fields and actions update only a few.

---

## 9. Bookkeeping/Tracking Pattern (Neutron DEX)

Maintain auxiliary data structures that track cumulative values for efficient
invariant checking. From the Neutron DEX specification.

```quint
// Track cumulative fees per liquidity position
var cumulativeFees: PoolId -> int
var lastClaimedFees: (Address, PoolId) -> int

pure def pendingFees(user: Address, pool: PoolId): int =
  cumulativeFees.getOrElse(pool, 0) - lastClaimedFees.getOrElse((user, pool), 0)

action collectFees(user: Address, pool: PoolId): bool = all {
  val fees = pendingFees(user, pool)
  fees > 0,
  balances' = balances.setBy(user, b => b + fees),
  lastClaimedFees' = lastClaimedFees.set((user, pool), cumulativeFees.get(pool)),
  cumulativeFees' = cumulativeFees,
}
```

**When to use:** DeFi protocols with accumulated rewards, fees, or interest.

---

## 10. False-Invariant Witnesses for Coverage

Write invariants that SHOULD be violated to prove the model is not vacuously
trivial. If these pass (no violation found), the model is too constrained.

```quint
// These should ALL be violated during simulation:

// Witness: some user can have a non-zero balance
val witnessNonZeroBalance = USERS.forall(u => balanceOf(u) == 0)

// Witness: a swap can actually execute
val witnessNoSwaps = swapCount == 0

// Witness: multiple actions are reachable
val witnessOnlySingleAction = ghostActionLog.length() <= 1

// Run: quint run --invariant=witnessNonZeroBalance spec.qnt
// Expected: VIOLATION FOUND (this means the model works!)
```

**When to use:** Always. Write at least one witness per major action to verify reachability.

---

## 11. Tolerance-Based Invariants (Timewave Vault)

For integer arithmetic with rounding, use tolerance bounds instead of exact equality.
From the Timewave Vault specification.

```quint
pure val ROUNDING_TOLERANCE = 1

// Instead of: shares * totalAssets / totalShares == expectedAssets
// Use: abs(shares * totalAssets / totalShares - expectedAssets) <= ROUNDING_TOLERANCE

val shareAccountingSound =
  USERS.forall(user =>
    val userShares = shares.getOrElse(user, 0)
    val expectedAssets = if (totalShares == 0) 0
      else userShares * totalAssets / totalShares
    val actualAssets = userAssets(user)
    abs(actualAssets - expectedAssets) <= ROUNDING_TOLERANCE
  )

// Rounding direction invariant: protocol never gives away free tokens
val roundingFavorsProtocol =
  USERS.forall(user =>
    sharesToAssets(assetsToShares(user.depositAmount)) <= user.depositAmount
  )
```

**When to use:** Vault share accounting, AMM price calculations, fee distributions,
any division-heavy arithmetic.

---

## 12. EVM/Smart Contract Environment Modeling (ZKSync Governance)

Model the smart contract execution environment including msg.sender, block context,
and contract storage. From the ZKSync Governance specification.

```quint
type CallContext = {
  msgSender: Address,
  blockNumber: int,
  blockTimestamp: int,
  txOrigin: Address,
}

var ctx: CallContext
var contractStorage: Address -> ContractState

action callContract(caller: Address, target: Address, calldata: Msg): bool = all {
  ctx' = { ...ctx, msgSender: caller },
  match calldata {
    | Deposit(d) => handleDeposit(target, d)
    | Withdraw(w) => handleWithdraw(target, w)
  },
}

// Model access control
pure def onlyOwner(ctx: CallContext, contract: ContractState): bool =
  ctx.msgSender == contract.owner
```

**When to use:** Modeling Solidity/EVM protocols, governance systems, access control.

---

## 13. Multi-Chain State with Per-Chain Maps (ICS-20, CCV)

Model multiple chains as a map from chain ID to chain state. Relayers operate
across chains nondeterministically.

```quint
type ChainId = str

type ChainState = {
  balances: Address -> (Denom -> int),
  escrow: Denom -> int,
  height: int,
  outbox: Set[Packet],
  inbox: Set[Packet],
}

var chains: ChainId -> ChainState

action relayPacket(srcChain: ChainId, dstChain: ChainId, packet: Packet): bool = all {
  // Packet is in source outbox
  chains.get(srcChain).outbox.contains(packet),
  // Move to destination inbox
  chains' = chains
    .setBy(srcChain, s => { ...s, outbox: s.outbox.exclude(Set(packet)) })
    .setBy(dstChain, d => { ...d, inbox: d.inbox.union(Set(packet)) }),
}
```

**When to use:** Any cross-chain protocol: IBC, bridges, cross-chain intents.

---

## 14. Packet Queue Pattern for Async Cross-Chain Communication

Model ordered packet delivery using lists as queues. Packets are appended on send
and consumed from the head on receive.

```quint
type Packet = {
  sequence: int,
  srcChannel: str,
  dstChannel: str,
  data: PacketData,
  timeoutHeight: int,
}

var packetQueues: (ChainId, ChannelId) -> List[Packet]
var nextSequenceSend: (ChainId, ChannelId) -> int
var nextSequenceRecv: (ChainId, ChannelId) -> int

action sendPacket(chain: ChainId, channel: ChannelId, data: PacketData, timeout: int): bool = {
  val seq = nextSequenceSend.getOrElse((chain, channel), 1)
  val packet = { sequence: seq, srcChannel: channel, dstChannel: counterparty(channel), data: data, timeoutHeight: timeout }
  all {
    packetQueues' = packetQueues.setBy((chain, channel), q => q.append(packet)),
    nextSequenceSend' = nextSequenceSend.set((chain, channel), seq + 1),
    // frame conditions
    nextSequenceRecv' = nextSequenceRecv,
  }
}

action receivePacket(chain: ChainId, channel: ChannelId): bool = {
  val queue = packetQueues.getOrElse((chain, channel), [])
  all {
    queue.length() > 0,
    val packet = queue.head()
    val expectedSeq = nextSequenceRecv.getOrElse((chain, channel), 1)
    packet.sequence == expectedSeq,
    processPacketData(chain, packet.data),
    packetQueues' = packetQueues.set((chain, channel), queue.tail()),
    nextSequenceRecv' = nextSequenceRecv.set((chain, channel), expectedSeq + 1),
    nextSequenceSend' = nextSequenceSend,
  }
}
```

**When to use:** IBC-style ordered channels, any sequenced message protocol.

---

## 15. Keeper/Module Namespacing (Cosmos Bank)

Organize specifications into keeper-like modules that mirror Cosmos SDK architecture.
Each module owns its state and exposes actions.

```quint
module BankKeeper {
  var balances: Address -> (Denom -> int)

  action sendCoins(from: Address, to: Address, denom: Denom, amount: int): bool = all {
    balances.getOrElse(from, Map()).getOrElse(denom, 0) >= amount,
    balances' = balances
      .setBy(from, b => b.setBy(denom, v => v - amount))
      .setBy(to, b => b.setBy(denom, v => v.getOrElse(0) + amount)),
  }

  action mintCoins(to: Address, denom: Denom, amount: int): bool = all {
    amount > 0,
    balances' = balances
      .setBy(to, b => b.setBy(denom, v => v.getOrElse(0) + amount)),
  }
}

module StakingKeeper {
  import BankKeeper.*

  var delegations: (Address, Address) -> int  // (delegator, validator) -> amount

  action delegate(delegator: Address, validator: Address, amount: int): bool = all {
    sendCoins(delegator, "staking_pool", "uatom", amount),
    delegations' = delegations.setBy((delegator, validator), d => d.getOrElse(0) + amount),
  }
}
```

**When to use:** Cosmos SDK module specifications, any modular protocol architecture.

---

## 16. Monotonic Counter Pattern

For values that only increase (block height, sequence numbers, nonces). Invariant
verifies monotonicity.

```quint
var blockHeight: int
var nonces: Address -> int

action advanceBlock: bool = all {
  blockHeight' = blockHeight + 1,
  nonces' = nonces,
}

val heightMonotonic = blockHeight >= 0
// Temporal: height always increases (checked across steps)
// In practice, check that each step's blockHeight' >= blockHeight
```

**When to use:** Sequence numbers, block heights, nonces, any monotonically increasing value.
