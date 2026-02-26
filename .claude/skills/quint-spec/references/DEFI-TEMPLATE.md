# DeFi Protocol Templates

Starter templates for common DeFi protocol patterns. Copy and adapt these as a
starting point for your specification.

---

## Token / Balance Accounting (Cosmos Bank Pattern)

The foundation for any protocol that manages token balances.

```quint
module BankTypes {
  type Address = str
  type Denom = str
  type Amount = int
  type Balances = Address -> (Denom -> Amount)
}

module Bank {
  import BankTypes.*

  const ADDRESSES: Set[Address]
  const DENOMS: Set[Denom]
  const MAX_AMOUNT: int

  var balances: Balances
  var totalSupply: Denom -> Amount

  pure def getBalance(bals: Balances, addr: Address, denom: Denom): Amount =
    bals.getOrElse(addr, Map()).getOrElse(denom, 0)

  action init = all {
    balances' = Map(),
    totalSupply' = Map(),
  }

  action mint(to: Address, denom: Denom, amount: Amount): bool = all {
    amount > 0,
    amount <= MAX_AMOUNT,
    balances' = balances.setBy(to, b =>
      b.getOrElse(Map()).setBy(denom, v => v.getOrElse(0) + amount)),
    totalSupply' = totalSupply.setBy(denom, s => s.getOrElse(0) + amount),
  }

  action burn(from: Address, denom: Denom, amount: Amount): bool = all {
    amount > 0,
    getBalance(balances, from, denom) >= amount,
    balances' = balances.setBy(from, b =>
      b.setBy(denom, v => v - amount)),
    totalSupply' = totalSupply.setBy(denom, s => s - amount),
  }

  action send(from: Address, to: Address, denom: Denom, amount: Amount): bool = all {
    amount > 0,
    from != to,
    getBalance(balances, from, denom) >= amount,
    balances' = balances
      .setBy(from, b => b.setBy(denom, v => v - amount))
      .setBy(to, b => b.getOrElse(Map()).setBy(denom, v => v.getOrElse(0) + amount)),
    totalSupply' = totalSupply,
  }

  action step = {
    nondet from = ADDRESSES.oneOf()
    nondet to = ADDRESSES.oneOf()
    nondet denom = DENOMS.oneOf()
    nondet amount = 1.to(MAX_AMOUNT).oneOf()
    any {
      mint(from, denom, amount),
      burn(from, denom, amount),
      send(from, to, denom, amount),
    }
  }

  // Invariants
  val supplyConserved = DENOMS.forall(d =>
    totalSupply.getOrElse(d, 0) ==
      ADDRESSES.fold(0, (sum, addr) => sum + getBalance(balances, addr, d))
  )

  val noNegativeBalances = ADDRESSES.forall(addr =>
    DENOMS.forall(d => getBalance(balances, addr, d) >= 0)
  )

  val noNegativeSupply = DENOMS.forall(d => totalSupply.getOrElse(d, 0) >= 0)
}
```

---

## AMM Pool (Constant Product)

Constant product market maker with swap fees.

```quint
module AMMTypes {
  type Address = str
  type Pool = {
    reserve0: int,
    reserve1: int,
    totalShares: int,
    feeNumerator: int,    // e.g., 3 for 0.3%
    feeDenominator: int,  // e.g., 1000
  }
}

module AMM {
  import AMMTypes.*

  const USERS: Set[Address]
  const MAX_AMOUNT: int

  var pool: Pool
  var lpShares: Address -> int
  var userBalance0: Address -> int
  var userBalance1: Address -> int

  action init = all {
    pool' = { reserve0: 0, reserve1: 0, totalShares: 0,
              feeNumerator: 3, feeDenominator: 1000 },
    lpShares' = Map(),
    userBalance0' = USERS.mapBy(u => 1000),
    userBalance1' = USERS.mapBy(u => 1000),
  }

  // Add liquidity (simplified: proportional deposits)
  action addLiquidity(user: Address, amount0: int, amount1: int): bool = all {
    amount0 > 0,
    amount1 > 0,
    userBalance0.getOrElse(user, 0) >= amount0,
    userBalance1.getOrElse(user, 0) >= amount1,
    val newShares = if (pool.totalShares == 0) amount0  // First LP
      else amount0 * pool.totalShares / pool.reserve0,
    newShares > 0,
    pool' = { ...pool,
      reserve0: pool.reserve0 + amount0,
      reserve1: pool.reserve1 + amount1,
      totalShares: pool.totalShares + newShares },
    lpShares' = lpShares.setBy(user, s => s.getOrElse(0) + newShares),
    userBalance0' = userBalance0.setBy(user, b => b - amount0),
    userBalance1' = userBalance1.setBy(user, b => b - amount1),
  }

  // Swap token0 for token1
  action swap0For1(user: Address, amountIn: int): bool = all {
    amountIn > 0,
    userBalance0.getOrElse(user, 0) >= amountIn,
    pool.reserve0 > 0,
    pool.reserve1 > 0,
    // Calculate output with fee
    val amountInAfterFee = amountIn * (pool.feeDenominator - pool.feeNumerator),
    val amountOut = amountInAfterFee * pool.reserve1 /
      (pool.reserve0 * pool.feeDenominator + amountInAfterFee),
    amountOut > 0,
    amountOut < pool.reserve1,
    pool' = { ...pool,
      reserve0: pool.reserve0 + amountIn,
      reserve1: pool.reserve1 - amountOut },
    userBalance0' = userBalance0.setBy(user, b => b - amountIn),
    userBalance1' = userBalance1.setBy(user, b => b + amountOut),
    lpShares' = lpShares,
  }

  action step = {
    nondet user = USERS.oneOf()
    nondet amount = 1.to(MAX_AMOUNT).oneOf()
    nondet amount2 = 1.to(MAX_AMOUNT).oneOf()
    any {
      addLiquidity(user, amount, amount2),
      swap0For1(user, amount),
    }
  }

  // k = reserve0 * reserve1 should never decrease (increases from fees)
  val kNonDecreasing = pool.reserve0 * pool.reserve1 >= 0

  // No negative reserves
  val reservesSolvent = pool.reserve0 >= 0 and pool.reserve1 >= 0
}
```

---

## ERC-4626 Vault (Share/Asset Conversion)

Tokenized vault with deposit/withdraw and share accounting.

```quint
module Vault {
  type Address = str

  const USERS: Set[Address]
  const MAX_DEPOSIT: int
  pure val ROUNDING_TOLERANCE = 1

  var totalAssets: int
  var totalShares: int
  var userShares: Address -> int
  var userAssets: Address -> int  // External balances

  pure def assetsToShares(assets: int, totAssets: int, totShares: int): int =
    if (totAssets == 0) assets
    else assets * totShares / totAssets

  pure def sharesToAssets(shares: int, totAssets: int, totShares: int): int =
    if (totShares == 0) 0
    else shares * totAssets / totShares

  action init = all {
    totalAssets' = 0,
    totalShares' = 0,
    userShares' = Map(),
    userAssets' = USERS.mapBy(u => 1000),
  }

  action deposit(user: Address, assets: int): bool = all {
    assets > 0,
    userAssets.getOrElse(user, 0) >= assets,
    val shares = assetsToShares(assets, totalAssets, totalShares),
    shares > 0,
    totalAssets' = totalAssets + assets,
    totalShares' = totalShares + shares,
    userShares' = userShares.setBy(user, s => s.getOrElse(0) + shares),
    userAssets' = userAssets.setBy(user, a => a - assets),
  }

  action withdraw(user: Address, shares: int): bool = all {
    shares > 0,
    userShares.getOrElse(user, 0) >= shares,
    val assets = sharesToAssets(shares, totalAssets, totalShares),
    assets > 0,
    totalAssets' = totalAssets - assets,
    totalShares' = totalShares - shares,
    userShares' = userShares.setBy(user, s => s - shares),
    userAssets' = userAssets.setBy(user, a => a + assets),
  }

  action step = {
    nondet user = USERS.oneOf()
    nondet amount = 1.to(MAX_DEPOSIT).oneOf()
    any {
      deposit(user, amount),
      withdraw(user, amount),
    }
  }

  // Share accounting: no free tokens from rounding
  val roundingFavorsVault = USERS.forall(user =>
    val s = userShares.getOrElse(user, 0)
    val roundTrip = sharesToAssets(assetsToShares(s, totalAssets, totalShares), totalAssets, totalShares)
    // Original shares -> assets -> shares should not gain value
    roundTrip <= s or totalShares == 0
  )

  // Solvency: vault always has enough assets to cover shares
  val vaultSolvent = totalAssets >= 0 and totalShares >= 0
}
```

---

## Lending Position (Health Factor)

Basic lending with collateral, borrowing, and liquidation.

```quint
module Lending {
  type Address = str

  const USERS: Set[Address]
  const COLLATERAL_FACTOR: int  // e.g., 150 = 150% collateralization
  const LIQUIDATION_BONUS: int  // e.g., 5 = 5%
  const PRICE_RANGE: Set[int]   // Possible oracle prices

  var collateral: Address -> int
  var borrows: Address -> int
  var oraclePrice: int          // Price of collateral in borrow terms

  pure def healthFactor(coll: int, debt: int, price: int): int =
    if (debt == 0) 99999  // Healthy if no debt
    else coll * price * 100 / debt

  action init = all {
    collateral' = Map(),
    borrows' = Map(),
    oraclePrice' = 100,
  }

  action depositCollateral(user: Address, amount: int): bool = all {
    amount > 0,
    collateral' = collateral.setBy(user, c => c.getOrElse(0) + amount),
    borrows' = borrows,
    oraclePrice' = oraclePrice,
  }

  action borrow(user: Address, amount: int): bool = all {
    amount > 0,
    val newDebt = borrows.getOrElse(user, 0) + amount,
    val coll = collateral.getOrElse(user, 0),
    healthFactor(coll, newDebt, oraclePrice) >= COLLATERAL_FACTOR,
    borrows' = borrows.set(user, newDebt),
    collateral' = collateral,
    oraclePrice' = oraclePrice,
  }

  action liquidate(liquidator: Address, user: Address): bool = all {
    val debt = borrows.getOrElse(user, 0),
    val coll = collateral.getOrElse(user, 0),
    debt > 0,
    healthFactor(coll, debt, oraclePrice) < COLLATERAL_FACTOR,
    // Liquidator repays debt, seizes collateral + bonus
    val seizedCollateral = debt * (100 + LIQUIDATION_BONUS) / (oraclePrice * 100),
    seizedCollateral <= coll,
    collateral' = collateral.set(user, coll - seizedCollateral),
    borrows' = borrows.set(user, 0),
    oraclePrice' = oraclePrice,
  }

  // Oracle price can change nondeterministically
  action priceChange: bool = {
    nondet newPrice = PRICE_RANGE.oneOf()
    all {
      oraclePrice' = newPrice,
      collateral' = collateral,
      borrows' = borrows,
    }
  }

  action step = {
    nondet user = USERS.oneOf()
    nondet amount = 1.to(100).oneOf()
    nondet liquidator = USERS.oneOf()
    any {
      depositCollateral(user, amount),
      borrow(user, amount),
      liquidate(liquidator, user),
      priceChange,
    }
  }

  // Protocol is always solvent: total collateral value >= total borrows
  val protocolSolvent =
    val totalColl = USERS.fold(0, (sum, u) => sum + collateral.getOrElse(u, 0))
    val totalDebt = USERS.fold(0, (sum, u) => sum + borrows.getOrElse(u, 0))
    totalColl * oraclePrice >= totalDebt * 100 or totalDebt == 0

  // No negative positions
  val noNegativePositions = USERS.forall(u =>
    collateral.getOrElse(u, 0) >= 0 and borrows.getOrElse(u, 0) >= 0
  )
}
```
