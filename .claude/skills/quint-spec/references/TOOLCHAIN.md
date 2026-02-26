# Quint Toolchain Reference

## Installation

```bash
# Install Quint CLI
npm install -g @informalsystems/quint

# Verify
quint --version
```

For formal verification (`quint verify`), Apalache is required:
- Requires JDK 17+
- Install: https://apalache-mc.org/docs/apalache/installation/jvm.html
- Apalache is automatically invoked by `quint verify`

## Commands

### quint typecheck

Type-check a Quint specification without running it.

```bash
quint typecheck spec.qnt
```

Catches type errors, undefined references, and arity mismatches. Always run
this first before simulation or verification.

### quint test

Run named test traces (actions prefixed with `run`).

```bash
# Run all tests in the file
quint test spec.qnt

# Run a specific test
quint test --match=happyPathTest spec.qnt
```

Tests are `run` declarations that chain actions with `.then()` and check
properties with `.expect()`. A test passes if it reaches the end without
failing an expectation or violating a guard.

### quint run

Simulate the specification by randomly executing the `step` action and
checking invariants after each step.

```bash
# Basic simulation
quint run --invariant=balancesConserved spec.qnt

# More thorough simulation
quint run --invariant=balancesConserved --max-samples=10000 --max-steps=50 spec.qnt

# Reproducible run with seed
quint run --invariant=balancesConserved --seed=42 spec.qnt

# Check multiple invariants
quint run --invariant=balancesConserved --invariant=noNegativeBalances spec.qnt

# Specify the init and step actions (if not named `init`/`step`)
quint run --init=myInit --step=myStep --invariant=myInvariant spec.qnt

# Run from a specific module
quint run --main=BankTest --invariant=supplyConserved spec.qnt
```

**Key flags:**
| Flag | Default | Description |
|------|---------|-------------|
| `--invariant` | (none) | Invariant(s) to check |
| `--max-samples` | 10000 | Number of random traces to explore |
| `--max-steps` | 20 | Maximum steps per trace |
| `--seed` | (random) | Random seed for reproducibility |
| `--init` | `init` | Name of the initialization action |
| `--step` | `step` | Name of the step action |
| `--main` | (last module) | Module to run |
| `--verbosity` | 2 | Output detail level (0-5) |

**Output:** If a violation is found, prints the trace (sequence of states)
leading to the violation. If no violation is found after all samples, reports
success (but this is NOT a proof -- use `quint verify` for that).

### quint verify

Formal verification using the Apalache model checker. Exhaustively explores
the reachable state space (bounded by `--max-steps`).

```bash
# Basic verification
quint verify --invariant=balancesConserved spec.qnt

# Bounded model checking (10 steps)
quint verify --invariant=balancesConserved --max-steps=10 spec.qnt

# Specify module
quint verify --main=BankTest --invariant=supplyConserved spec.qnt
```

**Key flags:**
| Flag | Default | Description |
|------|---------|-------------|
| `--invariant` | (none) | Invariant to verify |
| `--max-steps` | 10 | Maximum execution steps to explore |
| `--main` | (last module) | Module to verify |
| `--init` | `init` | Initialization action |
| `--step` | `step` | Step action |

**Output:** Either proves the invariant holds for all reachable states within
the bound, or produces a counterexample trace.

**Performance tips:**
- Start with small bounds (`--max-steps=5`) and increase
- Reduce constants (fewer addresses, smaller amounts) for faster checking
- Use `quint run` first to catch easy bugs before formal verification

### quint (REPL)

Interactive exploration of specifications.

```bash
# Start REPL
quint

# Load a file in REPL
.load spec.qnt

# Evaluate expressions
>>> 1 + 2
3
>>> Set(1, 2, 3).map(x => x * 2)
Set(2, 4, 6)

# Run an action
>>> init
true
>>> step
true

# Check a property
>>> balancesConserved
true

# Exit
.exit
```

The REPL is useful for:
- Exploring types and operators interactively
- Testing individual actions in isolation
- Debugging counterexample traces by replaying step-by-step

## Typical Workflow

```bash
# 1. Type-check
quint typecheck spec.qnt

# 2. Run tests
quint test spec.qnt

# 3. Quick simulation (find obvious bugs)
quint run --invariant=myInvariant --max-samples=1000 spec.qnt

# 4. Verify false-invariant witnesses (confirm model isn't vacuous)
quint run --invariant=witnessNoActivity spec.qnt
# Expected: violation found

# 5. Thorough simulation
quint run --invariant=myInvariant --max-samples=10000 --max-steps=50 spec.qnt

# 6. Formal verification (requires Apalache + JDK 17+)
quint verify --invariant=myInvariant --max-steps=10 spec.qnt
```
