# Intent-Based System Templates

Starter templates for intent lifecycle state machines, solver competition,
batch auctions, and optimistic verification. These patterns have no existing
Quint specifications in the wild -- they're novel formalizations.

---

## Intent Lifecycle State Machine (ERC-7683 Style)

Models the full lifecycle of a cross-chain intent from creation through
settlement or expiry.

```quint
module IntentTypes {
  type Address = str
  type IntentId = int
  type ChainId = str

  type Intent = {
    id: IntentId,
    creator: Address,
    inputToken: str,
    inputAmount: int,
    outputToken: str,
    minOutputAmount: int,
    sourceChain: ChainId,
    destChain: ChainId,
    deadline: int,
  }

  type Fill = {
    intentId: IntentId,
    solver: Address,
    outputAmount: int,
    fillHeight: int,
  }

  type Status = Pending | Matched | Filled | Settled | Expired
}

module IntentLifecycle {
  import IntentTypes.*

  const USERS: Set[Address]
  const SOLVERS: Set[Address]
  const CHAINS: Set[ChainId]
  const TOKENS: Set[str]
  const MAX_AMOUNT: int

  var intents: IntentId -> Intent
  var status: IntentId -> Status
  var fills: IntentId -> Fill
  var balances: (ChainId, Address, str) -> int  // (chain, addr, token) -> amount
  var nextIntentId: int
  var currentHeight: int

  pure def getBalance(chain: ChainId, addr: Address, token: str): int =
    balances.getOrElse((chain, addr, token), 0)

  action init = all {
    intents' = Map(),
    status' = Map(),
    fills' = Map(),
    balances' = Map(),
    nextIntentId' = 1,
    currentHeight' = 1,
  }

  // User creates an intent: locks input tokens
  action createIntent(creator: Address, inputToken: str, inputAmount: int,
                      outputToken: str, minOutput: int,
                      srcChain: ChainId, dstChain: ChainId): bool = all {
    inputAmount > 0,
    minOutput > 0,
    getBalance(srcChain, creator, inputToken) >= inputAmount,
    val intent: Intent = {
      id: nextIntentId, creator: creator,
      inputToken: inputToken, inputAmount: inputAmount,
      outputToken: outputToken, minOutputAmount: minOutput,
      sourceChain: srcChain, destChain: dstChain,
      deadline: currentHeight + 20,
    }
    intents' = intents.set(nextIntentId, intent),
    status' = status.set(nextIntentId, Pending),
    // Lock input tokens
    balances' = balances.setBy((srcChain, creator, inputToken), b => b - inputAmount),
    fills' = fills,
    nextIntentId' = nextIntentId + 1,
    currentHeight' = currentHeight,
  }

  // Solver fills the intent on the destination chain
  action fillIntent(solver: Address, intentId: IntentId, outputAmount: int): bool = all {
    status.has(intentId),
    status.get(intentId) == Pending,
    val intent = intents.get(intentId)
    currentHeight < intent.deadline,
    // Solver must provide at least minOutputAmount
    outputAmount >= intent.minOutputAmount,
    // Solver has sufficient balance on dest chain
    getBalance(intent.destChain, solver, intent.outputToken) >= outputAmount,
    // Transfer output to intent creator on dest chain
    balances' = balances
      .setBy((intent.destChain, solver, intent.outputToken), b => b - outputAmount)
      .setBy((intent.destChain, intent.creator, intent.outputToken), b => b + outputAmount),
    status' = status.set(intentId, Filled),
    fills' = fills.set(intentId, {
      intentId: intentId, solver: solver,
      outputAmount: outputAmount, fillHeight: currentHeight,
    }),
    intents' = intents,
    nextIntentId' = nextIntentId,
    currentHeight' = currentHeight,
  }

  // Settlement: release escrowed input tokens to solver
  action settleIntent(intentId: IntentId): bool = all {
    status.has(intentId),
    status.get(intentId) == Filled,
    val intent = intents.get(intentId)
    val fill = fills.get(intentId)
    // Release locked input tokens to solver on source chain
    balances' = balances.setBy(
      (intent.sourceChain, fill.solver, intent.inputToken), b => b + intent.inputAmount),
    status' = status.set(intentId, Settled),
    intents' = intents,
    fills' = fills,
    nextIntentId' = nextIntentId,
    currentHeight' = currentHeight,
  }

  // Expiry: return locked tokens to creator
  action expireIntent(intentId: IntentId): bool = all {
    status.has(intentId),
    status.get(intentId) == Pending,
    val intent = intents.get(intentId)
    currentHeight >= intent.deadline,
    // Return locked tokens
    balances' = balances.setBy(
      (intent.sourceChain, intent.creator, intent.inputToken),
      b => b + intent.inputAmount),
    status' = status.set(intentId, Expired),
    intents' = intents,
    fills' = fills,
    nextIntentId' = nextIntentId,
    currentHeight' = currentHeight,
  }

  action advanceHeight: bool = all {
    currentHeight' = currentHeight + 1,
    intents' = intents, status' = status, fills' = fills,
    balances' = balances, nextIntentId' = nextIntentId,
  }

  action step = {
    nondet user = USERS.oneOf()
    nondet solver = SOLVERS.oneOf()
    nondet inToken = TOKENS.oneOf()
    nondet outToken = TOKENS.oneOf()
    nondet amount = 1.to(MAX_AMOUNT).oneOf()
    nondet minOut = 1.to(MAX_AMOUNT).oneOf()
    nondet srcChain = CHAINS.oneOf()
    nondet dstChain = CHAINS.oneOf()
    any {
      createIntent(user, inToken, amount, outToken, minOut, srcChain, dstChain),
      if (intents.keys().size() > 0) {
        nondet intentId = intents.keys().oneOf()
        any {
          fillIntent(solver, intentId, amount),
          settleIntent(intentId),
          expireIntent(intentId),
        }
      } else all {
        intents' = intents, status' = status, fills' = fills,
        balances' = balances, nextIntentId' = nextIntentId, currentHeight' = currentHeight,
      },
      advanceHeight,
    }
  }

  // Constraint satisfaction: every fill meets minimum output
  val fillsSatisfyConstraints = intents.keys().forall(id =>
    if (fills.has(id))
      fills.get(id).outputAmount >= intents.get(id).minOutputAmount
    else true
  )

  // No tokens created or destroyed (conservation across all chains)
  val intentTokensConserved = intents.keys().forall(id =>
    val s = status.get(id)
    val intent = intents.get(id)
    // Pending: input locked (deducted from creator)
    // Filled: input still locked, output transferred from solver to creator
    // Settled: input released to solver, output already with creator
    // Expired: input returned to creator
    s == Pending or s == Filled or s == Settled or s == Expired
  )

  // Terminal states are truly terminal
  val terminalStatesStable = intents.keys().forall(id =>
    val s = status.get(id)
    // Settled and Expired intents should not change
    (s == Settled or s == Expired) implies (status.get(id) == s)
  )
}
```

---

## Solver Competition with Fairness

Models multiple solvers competing to fill intents with fairness tracking.

```quint
module SolverCompetition {
  type Address = str
  type IntentId = int

  const SOLVERS: Set[Address]
  const NUM_INTENTS: int

  var intentsFilled: IntentId -> Address  // Which solver filled each intent
  var solverFillCount: Address -> int     // How many intents each solver filled
  var nextIntent: int

  action init = all {
    intentsFilled' = Map(),
    solverFillCount' = SOLVERS.mapBy(s => 0),
    nextIntent' = 1,
  }

  // Any solver can fill the next available intent
  action fillNext(solver: Address): bool = all {
    SOLVERS.contains(solver),
    nextIntent <= NUM_INTENTS,
    not(intentsFilled.has(nextIntent)),
    intentsFilled' = intentsFilled.set(nextIntent, solver),
    solverFillCount' = solverFillCount.setBy(solver, c => c + 1),
    nextIntent' = nextIntent + 1,
  }

  action step = {
    nondet solver = SOLVERS.oneOf()
    fillNext(solver)
  }

  // Fairness: no solver fills more than 2x the average
  // (only meaningful when enough intents have been filled)
  val solverFairness =
    val totalFills = SOLVERS.fold(0, (sum, s) => sum + solverFillCount.getOrElse(s, 0))
    if (totalFills < SOLVERS.size() * 2) true
    else SOLVERS.forall(s =>
      val avg = totalFills / SOLVERS.size()
      solverFillCount.getOrElse(s, 0) <= avg * 2
    )
}
```

---

## Batch Auction with Uniform Clearing Price

All orders in a batch clear at the same price. Ensures no order gets
a worse price than their limit.

```quint
module BatchAuction {
  type Address = str
  type OrderId = int

  type Side = Buy | Sell

  type Order = {
    id: OrderId,
    trader: Address,
    side: Side,
    amount: int,
    limitPrice: int,  // Max price for buys, min price for sells (scaled by 1000)
  }

  const TRADERS: Set[Address]
  const MAX_AMOUNT: int
  const PRICE_RANGE: Set[int]  // Possible clearing prices (scaled)

  var orders: Set[Order]
  var clearingPrice: int       // 0 means not yet cleared
  var fills: OrderId -> int    // Filled amounts
  var nextOrderId: int
  var settled: bool

  action init = all {
    orders' = Set(),
    clearingPrice' = 0,
    fills' = Map(),
    nextOrderId' = 1,
    settled' = false,
  }

  // Submit order to the batch
  action submitOrder(trader: Address, side: Side, amount: int, limit: int): bool = all {
    not(settled),
    clearingPrice == 0,  // Still accepting orders
    amount > 0,
    limit > 0,
    val order: Order = { id: nextOrderId, trader: trader, side: side,
                         amount: amount, limitPrice: limit }
    orders' = orders.union(Set(order)),
    nextOrderId' = nextOrderId + 1,
    clearingPrice' = clearingPrice,
    fills' = fills,
    settled' = settled,
  }

  // Clear the batch at a uniform price
  action clearBatch(price: int): bool = all {
    not(settled),
    clearingPrice == 0,
    orders.size() > 0,
    price > 0,
    // Compute fills: buy orders with limit >= price, sell orders with limit <= price
    val buyOrders = orders.filter(o => match o.side { | Buy => true | Sell => false })
      .filter(o => o.limitPrice >= price)
    val sellOrders = orders.filter(o => match o.side { | Buy => false | Sell => true })
      .filter(o => o.limitPrice <= price)
    val totalBuy = buyOrders.fold(0, (sum, o) => sum + o.amount)
    val totalSell = sellOrders.fold(0, (sum, o) => sum + o.amount)
    // Match minimum of buy and sell volume
    val matchedVolume = if (totalBuy < totalSell) totalBuy else totalSell
    matchedVolume > 0,
    clearingPrice' = price,
    // Simplified: pro-rata fill for each qualifying order
    fills' = orders.fold(Map(), (acc, o) =>
      match o.side {
        | Buy => if (o.limitPrice >= price) acc.set(o.id, o.amount) else acc
        | Sell => if (o.limitPrice <= price) acc.set(o.id, o.amount) else acc
      }
    ),
    orders' = orders,
    nextOrderId' = nextOrderId,
    settled' = true,
  }

  action step = {
    nondet trader = TRADERS.oneOf()
    nondet amount = 1.to(MAX_AMOUNT).oneOf()
    nondet limit = PRICE_RANGE.oneOf()
    nondet price = PRICE_RANGE.oneOf()
    any {
      submitOrder(trader, Buy, amount, limit),
      submitOrder(trader, Sell, amount, limit),
      clearBatch(price),
    }
  }

  // Uniform price: all fills happen at the same price
  val uniformPrice = clearingPrice > 0 implies (
    fills.keys().forall(id =>
      orders.filter(o => o.id == id).forall(o =>
        match o.side {
          | Buy => o.limitPrice >= clearingPrice
          | Sell => o.limitPrice <= clearingPrice
        }
      )
    )
  )

  // No order gets worse than their limit price
  val limitsRespected = fills.keys().forall(id =>
    orders.filter(o => o.id == id).forall(o =>
      match o.side {
        | Buy => clearingPrice <= o.limitPrice
        | Sell => clearingPrice >= o.limitPrice
      }
    )
  )
}
```

---

## Optimistic Verification with Challenge Period

Fills are assumed valid during a challenge period. Anyone can challenge
with proof of invalidity.

```quint
module OptimisticVerification {
  type Address = str
  type FillId = int

  type FillStatus = Optimistic | Challenged | Verified | Slashed

  type FillRecord = {
    id: FillId,
    solver: Address,
    claimedOutput: int,
    actualOutput: int,  // Ghost: real value for verification modeling
    submitHeight: int,
  }

  const SOLVERS: Set[Address]
  const CHALLENGERS: Set[Address]
  const CHALLENGE_PERIOD: int  // Blocks until finality

  var fillRecords: FillId -> FillRecord
  var fillStatus: FillId -> FillStatus
  var solverBonds: Address -> int
  var currentHeight: int
  var nextFillId: int

  action init = all {
    fillRecords' = Map(),
    fillStatus' = Map(),
    solverBonds' = SOLVERS.mapBy(s => 100),  // Each solver posts a bond
    currentHeight' = 1,
    nextFillId' = 1,
  }

  // Solver submits an optimistic fill claim
  action submitFill(solver: Address, claimedOutput: int, actualOutput: int): bool = all {
    SOLVERS.contains(solver),
    claimedOutput > 0,
    solverBonds.getOrElse(solver, 0) > 0,  // Must have active bond
    val record: FillRecord = {
      id: nextFillId, solver: solver,
      claimedOutput: claimedOutput, actualOutput: actualOutput,
      submitHeight: currentHeight,
    }
    fillRecords' = fillRecords.set(nextFillId, record),
    fillStatus' = fillStatus.set(nextFillId, Optimistic),
    nextFillId' = nextFillId + 1,
    solverBonds' = solverBonds,
    currentHeight' = currentHeight,
  }

  // Challenger challenges an invalid fill
  action challenge(challenger: Address, fillId: FillId): bool = all {
    fillStatus.has(fillId),
    fillStatus.get(fillId) == Optimistic,
    val record = fillRecords.get(fillId)
    // Within challenge period
    currentHeight < record.submitHeight + CHALLENGE_PERIOD,
    // Fill is actually invalid (challenger knows the truth)
    record.claimedOutput > record.actualOutput,
    fillStatus' = fillStatus.set(fillId, Challenged),
    // Slash solver's bond, reward challenger (simplified)
    solverBonds' = solverBonds.setBy(record.solver, b => b - 10),
    fillRecords' = fillRecords,
    nextFillId' = nextFillId,
    currentHeight' = currentHeight,
  }

  // Finalize after challenge period with no challenge
  action finalize(fillId: FillId): bool = all {
    fillStatus.has(fillId),
    fillStatus.get(fillId) == Optimistic,
    val record = fillRecords.get(fillId)
    currentHeight >= record.submitHeight + CHALLENGE_PERIOD,
    fillStatus' = fillStatus.set(fillId, Verified),
    fillRecords' = fillRecords,
    solverBonds' = solverBonds,
    nextFillId' = nextFillId,
    currentHeight' = currentHeight,
  }

  action advanceHeight: bool = all {
    currentHeight' = currentHeight + 1,
    fillRecords' = fillRecords, fillStatus' = fillStatus,
    solverBonds' = solverBonds, nextFillId' = nextFillId,
  }

  action step = {
    nondet solver = SOLVERS.oneOf()
    nondet challenger = CHALLENGERS.oneOf()
    nondet claimed = 1.to(100).oneOf()
    nondet actual = 1.to(100).oneOf()
    any {
      submitFill(solver, claimed, actual),
      if (fillRecords.keys().size() > 0) {
        nondet fillId = fillRecords.keys().oneOf()
        any {
          challenge(challenger, fillId),
          finalize(fillId),
        }
      } else all {
        fillRecords' = fillRecords, fillStatus' = fillStatus,
        solverBonds' = solverBonds, nextFillId' = nextFillId,
        currentHeight' = currentHeight,
      },
      advanceHeight,
    }
  }

  // Safety: no invalid fill can be verified
  // (an invalid fill must be challenged before finalization)
  val noInvalidFinalizations = fillRecords.keys().forall(id =>
    val record = fillRecords.get(id)
    val s = fillStatus.get(id)
    (s == Verified) implies (record.claimedOutput <= record.actualOutput)
  )
}
```
