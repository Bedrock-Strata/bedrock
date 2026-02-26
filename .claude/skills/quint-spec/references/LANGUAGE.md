# Quint Language Quick Reference

## Types

### Basic Types

```quint
int           // Arbitrary precision integer
bool          // true, false
str           // String literal: "hello"
```

### Collection Types

```quint
Set[T]        // Unordered, unique elements: Set(1, 2, 3)
List[T]       // Ordered sequence: [1, 2, 3]
Map[K, V]     // Key-value store: Map("a" -> 1, "b" -> 2)
(T1, T2)      // Tuple: (1, "hello")
```

### Record Types

```quint
// Named fields
type Pool = { reserve0: int, reserve1: int, k: int }

// Construction
val p: Pool = { reserve0: 100, reserve1: 200, k: 20000 }

// Access
p.reserve0          // 100

// Spread update (creates new record with updated fields)
{ ...p, reserve0: 150 }
```

### Sum Types (Variants)

```quint
type Option[T] = Some(T) | None
type Result[T, E] = Ok(T) | Err(E)

type Msg =
  | Deposit({ sender: str, amount: int })
  | Withdraw({ sender: str, shares: int })
  | Swap({ sender: str, tokenIn: str, amountIn: int })
```

### Type Aliases

```quint
type Address = str
type Denom = str
type Amount = int
type Balances = Address -> (Denom -> Amount)
```

## Qualifiers

### pure val / pure def

No state access. Compile-time constants and pure functions.

```quint
pure val MAX_SUPPLY = 1000000
pure def min(a: int, b: int): int = if (a < b) a else b
pure def abs(x: int): int = if (x >= 0) x else -x
```

### val / def

Can read state (no primes). Used for derived values and invariants.

```quint
val totalBalance = ADDRESSES.fold(0, (sum, a) => sum + balances.getOrElse(a, 0))
def balanceOf(addr: Address): int = balances.getOrElse(addr, 0)
```

### action

Can read and write state (primes allowed). Represents state transitions.

```quint
action deposit(sender: Address, amount: int): bool = all {
  amount > 0,
  balances' = balances.setBy(sender, b => b + amount),
  totalDeposits' = totalDeposits + amount,
}
```

### temporal

For temporal logic properties (liveness, fairness).

```quint
temporal eventuallySettled = eventually(status == "settled")
temporal alwaysConserved = always(balancesConserved)
```

## State Updates

### Primed Variables

The `'` (prime) suffix denotes the next-state value of a variable.

```quint
var counter: int

action increment = all {
  counter' = counter + 1,
}
```

**Rule:** Every action must assign ALL `var` variables. If unchanged:

```quint
action incrementOnlyCounter = all {
  counter' = counter + 1,
  otherVar' = otherVar,    // Frame condition: explicitly unchanged
}
```

## Action Composition

### all { ... } -- Conjunction

All conditions must hold and all updates apply atomically.

```quint
action transfer(from: Address, to: Address, amount: int): bool = all {
  balances.getOrElse(from, 0) >= amount,   // guard
  amount > 0,                               // guard
  balances' = balances                      // update
    .setBy(from, b => b - amount)
    .setBy(to, b => b.getOrElse(0) + amount),
}
```

### any { ... } -- Disjunction

Nondeterministic choice: exactly one branch is taken.

```quint
action step = any {
  deposit(sender, amount),
  withdraw(sender, shares),
  swap(sender, tokenIn, amountIn),
}
```

### nondet -- Nondeterministic Value Selection

Selects a value nondeterministically from a set. Model checker explores all choices.

```quint
action step = {
  nondet sender = ADDRESSES.oneOf()
  nondet amount = 1.to(100).oneOf()
  any {
    deposit(sender, amount),
    withdraw(sender, amount),
  }
}
```

## Pattern Matching

### match Expression

```quint
match msg {
  | Deposit(d) => handleDeposit(d.sender, d.amount)
  | Withdraw(w) => handleWithdraw(w.sender, w.shares)
  | Swap(s) => handleSwap(s.sender, s.tokenIn, s.amountIn)
}
```

### if-then-else

```quint
if (balance >= amount) Ok(balance - amount) else Err(InsufficientBalance)
```

## Module System

### Module Definition

```quint
module BankTypes {
  type Address = str
  type Amount = int
}
```

### Import

```quint
import BankTypes.*                    // Import all from module
import BankTypes.Address              // Import specific type
import BankTypes as BT                // Qualified import: BT.Address
```

### Export

```quint
module Facade {
  import BankModule.*
  export BankModule.*                 // Re-export for downstream consumers
}
```

### Instance with Constants

Parameterized modules are instantiated with concrete constants.

```quint
module BankModule {
  const ADDRESSES: Set[str]
  const DENOMS: Set[str]
  // ... state and actions using constants
}

module BankTest {
  import BankModule(
    ADDRESSES = Set("alice", "bob", "carol"),
    DENOMS = Set("uatom", "uosmo"),
  ).*
}
```

## Built-in Operators

### Integer

```quint
a + b, a - b, a * b, a / b, a % b   // Arithmetic
a == b, a != b                        // Equality
a < b, a <= b, a > b, a >= b         // Comparison
i.to(j)                              // Range set: {i, i+1, ..., j}
```

### Boolean

```quint
a and b, a or b, not(a)              // Logical
a implies b                           // Implication
a iff b                               // Biconditional
```

### Set

```quint
Set(1, 2, 3)                         // Literal
s.contains(x)                        // Membership
s.union(t), s.intersect(t)           // Set operations
s.exclude(t)                         // Difference: s \ t
s.filter(x => predicate)             // Filter
s.map(x => f(x))                     // Map
s.fold(init, (acc, x) => ...)        // Fold/reduce
s.forall(x => predicate)             // Universal quantifier
s.exists(x => predicate)             // Existential quantifier
s.size()                             // Cardinality
s.oneOf()                            // Nondeterministic choice (in nondet)
s.powerset()                         // Power set
s.flatten()                          // Flatten Set[Set[T]] -> Set[T]
```

### List

```quint
[1, 2, 3]                            // Literal
l.length()                           // Length
l.nth(i)                             // Element at index (0-based)
l.head()                             // First element
l.tail()                             // All except first
l.append(x)                          // Append to end
l.concat(m)                          // Concatenate lists
l.indices()                          // Set of valid indices
l.foldl(init, (acc, x) => ...)       // Left fold
l.select(x => predicate)             // Filter
l.slice(from, to)                    // Sublist [from, to)
```

### Map

```quint
Map("a" -> 1, "b" -> 2)              // Literal
m.get(key)                           // Get (fails if missing!)
m.getOrElse(key, default)            // Safe get
m.has(key)                           // Key exists
m.set(key, value)                    // Update/insert (returns new map)
m.setBy(key, f)                      // Update by function: m.setBy(k, v => v + 1)
m.keys()                             // Set of keys
m.mapBy(keys, k => v)                // Build map from key set
m.put(key, value)                    // Same as set
```

### Temporal (for verification)

```quint
always(p)                             // p holds in all states
eventually(p)                         // p holds in some future state
next(p)                               // p holds in the next state
enabled(action)                       // action's guards are satisfied
```

## Run Traces (Tests)

```quint
run myTest =
  init
    .then(action1(arg1, arg2))
    .expect(property1)
    .then(action2(arg3))
    .expect(property2)
    .fail()                           // Expect the last action to fail
```

## Common Idioms

```quint
// Safe balance lookup (nested map)
pure def getBalance(bals: Address -> (Denom -> int), addr: Address, denom: Denom): int =
  bals.getOrElse(addr, Map()).getOrElse(denom, 0)

// Require pattern (guard helper)
pure def require(cond: bool): bool = cond

// Integer set range for nondeterminism
nondet amount = 1.to(MAX_AMOUNT).oneOf()

// Tuple destructuring
val (x, y) = myTuple
```
