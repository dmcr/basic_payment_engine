# Implementation Plan — Payment Engine (Milestone 2: Engine + Domain Types)

## Context

Greenfield Rust project (edition 2024, only a Hello-World `src/main.rs`, no deps). The canonical specification is the four docs already on disk: `DEVLOG.md`, `ASSUMPTIONS.md`, `context/domain-model.md`, `context/state-machine.md`.

This plan is intentionally narrow: it covers the **domain types** and the **engine** — the pure, in-memory state machine that consumes well-formed `Tx` values and produces `Account` state. CSV parsing, the I/O wiring in `main`, stderr surfacing of ignores, output sorting, and end-to-end golden-file integration tests are **out of scope** and will be addressed in the next milestone's plan. The engine is designed so that those concerns can plug in without engine changes.

The engine is fully unit-testable on `Tx` values without any CSV. That separation is the load-bearing architectural decision in this plan.

## Spec gaps surfaced from canonical docs

These are points where the four canonical docs are silent or implicit. The plan resolves each — call out at exit-plan if a resolution is wrong.

- **Engine signal for ignores.** `state-machine.md` defines what gets ignored, not how the engine reports which rule fired. Plan: `Engine::apply` returns `Result<(), IgnoreReason>`. Production callers may drop `Err(_)`; the next milestone's I/O layer surfaces it on stderr; tests assert the exact reason. Aligns with the user's stated wish to retain visibility into ignored rows.
- **Internal `Tx` shape.** `domain-model.md` describes the *CSV row* as `amount: Option<Decimal>`; the engine's natural input is a refinement where each kind carries only what it needs. Plan: refine to a sum type with arm-specific shapes. The CSV/parser layer (next milestone) is the boundary that produces validated `Tx` values; the engine never inspects an `Option<Decimal>`.
- **`AlreadyDisputed`.** State-machine FSM has no edge for `dispute` on `Disputed`. By "no matching transition = ignored" + `ASSUMPTIONS.md` "disputed only once", it is ignored. Plan: distinct `IgnoreReason::AlreadyDisputed` variant (per session decision), separate from `WrongTxState`.
- **Withdrawal as disputed-tx target.** Storage retains only deposits, so a dispute-family row referencing a withdrawal `tx` is structurally indistinguishable from one referencing a non-existent `tx`. Plan: both surface as `IgnoreReason::UnknownTx`; no `WrongTxKind` variant (per session decision; YAGNI).
- **Duplicate `tx` for a deposit.** `ASSUMPTIONS.md` states tx ids are globally unique, so this is an input-contract violation, not an engine concern. Plan: trust the assumption; do not add a runtime check.
- **Output ordering.** `state-machine.md` leaves it unspecified, and ordering is **permanently out of scope**, not a requirement. Plan: engine exposes accounts via `accounts()` in whatever order the underlying `HashMap` yields; the I/O layer emits in iterator order. No sort, ever.
- **Internal precision.** `Decimal` math is exact at engine layer; rounding to 4dp is a *display* concern owned by the next milestone's writer. The engine stores full-precision `available` and `held`.
- **Engine ↔ I/O contract for ignored rows.** The engine is a pure function: `apply` returns `Result<(), IgnoreReason>`. The *caller* decides the side effect (stderr, log, drop, store). Concretely, the next milestone's `main` will do `if let Err(r) = engine.apply(tx) { eprintln!("ignored: …{:?}", r); }`. The engine itself never writes to stderr. This split keeps unit tests programmatic (assert exact reason, no stderr capture), keeps the engine embeddable in non-CLI contexts, and is the load-bearing reason `apply` returns a typed `Result` rather than unit. The stderr *contract* is delivered here (the typed enum); the stderr *call site* ships next milestone with the rest of `main`.

## Project layout (this milestone)

```
src/
  lib.rs            crate root + module wiring + re-exports
  domain.rs         Tx, TxKind, Account, Decimal alias
  engine.rs         Engine, the five handlers, IgnoreReason, internal DepositRecord
  main.rs           untouched this milestone (Hello-World stub remains)
tests/
  engine.rs         every FSM transition + every ignored-input case, driving Engine::apply directly
```

Next milestone will add `src/csv_io.rs`, rewire `main.rs`, and add a `tests/integration.rs` (or similar) with golden CSV fixtures.

`Cargo.toml` deps for **this** milestone: `rust_decimal` only. `csv` and `serde` join the deps in the next milestone with the I/O work — keep this milestone's surface minimal.

## Public types

```rust
// domain.rs

pub type Decimal = rust_decimal::Decimal;

pub enum Tx {
    Deposit    { client: u16, tx: u32, amount: Decimal },
    Withdrawal { client: u16, tx: u32, amount: Decimal },
    Dispute    { client: u16, tx: u32 },
    Resolve    { client: u16, tx: u32 },
    Chargeback { client: u16, tx: u32 },
}

pub struct Account {
    pub client:    u16,
    pub available: Decimal,
    pub held:      Decimal,
    pub locked:    bool,
}
impl Account {
    pub fn total(&self) -> Decimal { self.available + self.held }
}

// engine.rs

pub struct Engine { /* private fields */ }

impl Engine {
    pub fn new() -> Self;
    pub fn apply(&mut self, tx: Tx) -> Result<(), IgnoreReason>;
    pub fn accounts(&self) -> impl Iterator<Item = &Account>;  // HashMap iteration order; ordering is unspecified by spec and never sorted
}

pub enum IgnoreReason {
    AccountFrozen,
    InsufficientFunds,
    UnknownTx,
    WrongTxState,     // resolve/chargeback on non-Disputed; dispute on terminal
    AlreadyDisputed,  // dispute on Disputed
    ClientMismatch,
}
```

Internal-only (in `engine.rs`):

```rust
struct DepositRecord { client: u16, amount: Decimal, state: DepositState }
enum DepositState { Posted, Disputed, Resolved, ChargedBack }

// Engine state:
//   accounts: HashMap<u16, Account>     // O(1) per-row access
//   deposits: HashMap<u32, DepositRecord>  // O(1) tx lookup for dispute family
```

## Engine input/output flow

The engine is a synchronous, in-memory function over a stream of `Tx`. Per row:

1. Caller constructs a well-formed `Tx` (parser is responsible for validation; engine trusts).
2. Caller invokes `engine.apply(tx)`.
3. `apply` dispatches on the `Tx` variant to one of five handlers.
4. Result is `Ok(())` (state mutated) or `Err(IgnoreReason)` (state unchanged).

After the stream ends, `engine.accounts()` returns an iterator over all known accounts in `HashMap` order. The next milestone's writer emits them as-is — output ordering is permanently unspecified per `state-machine.md`.

## Five row-handler shape

Common skeleton for every handler:

1. `accounts.entry(client).or_insert_with(Account::zero_for(client))` — account-creation rule (`state-machine.md`: "first well-formed row" creates the account, regardless of whether subsequent guards pass).
2. If `account.locked` → `Err(AccountFrozen)`.
3. Variant-specific guards (table below).
4. Mutate balances and/or `DepositRecord` state.
5. `Ok(())`.

| Handler         | Specific guards (after frozen check)                                                                                                              | Effect on success                                                              |
|-----------------|---------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| `on_deposit`    | none                                                                                                                                              | `available += amount`; insert `DepositRecord{client, amount, Posted}` at `tx`. |
| `on_withdrawal` | `account.available >= amount` else `InsufficientFunds`.                                                                                           | `available -= amount`. (No `DepositRecord` written.)                           |
| `on_dispute`    | `deposits[tx]` exists else `UnknownTx`; `record.client == row.client` else `ClientMismatch`; state is `Posted` else `AlreadyDisputed` (if `Disputed`) / `WrongTxState` (if terminal). | `available -= amount`; `held += amount`; `state = Disputed`.                   |
| `on_resolve`    | `deposits[tx]` exists else `UnknownTx`; `record.client == row.client` else `ClientMismatch`; state is `Disputed` else `WrongTxState`.             | `available += amount`; `held -= amount`; `state = Resolved`.                   |
| `on_chargeback` | `deposits[tx]` exists else `UnknownTx`; `record.client == row.client` else `ClientMismatch`; state is `Disputed` else `WrongTxState`.             | `held -= amount`; `state = ChargedBack`; **`account.locked = true`**.          |

Notes:

- The dispute family looks up `record.amount`; the dispute/resolve/chargeback row itself carries no amount.
- The `client` on the *row* is matched against the `client` on the stored *deposit*, not against the account itself. This is what `state-machine.md`'s client-match guard says.
- Account creation in step 1 happens *before* the frozen check on purpose — but a frozen account already exists (it had to deposit + chargeback to get there), so the `or_insert_with` is a no-op in that case. The creation rule only "fires" on first contact for a given client.

## Error / ignore handling

The engine layer is total and pure: no I/O, no panics on normal control flow, no parsing. The only error channel is `IgnoreReason`. There are no recoverable runtime errors at this layer.

| Class                | Engine policy                                                |
|----------------------|--------------------------------------------------------------|
| Engine ignore        | Return `Err(IgnoreReason)` from `apply`; state untouched.    |
| I/O / parse / argv   | Not the engine's concern — owned by next milestone.          |
| Programmer error     | A `panic!` is a bug, never a runtime expectation.            |

## Test strategy (unit-test matrix)

All tests live in `tests/engine.rs`, drive `Engine::apply` directly with constructed `Tx` values, and assert: post-state of `accounts()`, post-state of the deposit lifecycle when relevant (via observable behaviour — e.g. follow-up rows), the exact `IgnoreReason` for negative cases, and the `total == available + held` invariant after every accepted op (helper).

A small `decimal!(...)` helper or `Decimal::from_str_exact` keeps the test bodies readable.

### Transition coverage — every edge in `state-machine.md`

| ID  | Edge                                | Scenario                                                                                  |
|-----|-------------------------------------|-------------------------------------------------------------------------------------------|
| T1  | (n/a) → Posted                      | Single deposit creates account, `available == amount`, `held == 0`, `locked == false`.    |
| T2  | Posted → Disputed                   | deposit then dispute; balances shift available→held; total unchanged.                     |
| T3  | Disputed → Resolved                 | deposit, dispute, resolve; balances return; total unchanged; account not locked.          |
| T4  | Disputed → ChargedBack              | deposit, dispute, chargeback; held decreases by amount; total decreases.                  |
| T5  | Active → Frozen (cross-machine)     | After T4, `account.locked == true`. Subsequent deposit returns `AccountFrozen`.           |
| W   | Withdrawal (non-stateful)           | deposit then withdrawal; available decreases; total decreases.                            |
| AC  | Account creation by non-deposit     | Single dispute against unknown tx → `UnknownTx`; client appears in `accounts()` with zeros. |

### Ignored-input coverage — every row in `state-machine.md`'s "Ignored input"

| Case                                         | Setup                                       | Expected `IgnoreReason` |
|----------------------------------------------|---------------------------------------------|-------------------------|
| Deposit on frozen account                    | force frozen; deposit                       | `AccountFrozen`         |
| Withdrawal on frozen account                 | force frozen; withdrawal                    | `AccountFrozen`         |
| Dispute on frozen account                    | force frozen; dispute                       | `AccountFrozen`         |
| Resolve on frozen account                    | force frozen; resolve                       | `AccountFrozen`         |
| Chargeback on frozen account                 | force frozen; chargeback                    | `AccountFrozen`         |
| Withdrawal exceeds available                 | deposit 1.0; withdraw 2.0                   | `InsufficientFunds`     |
| Dispute on unknown tx                        | dispute alone                               | `UnknownTx`             |
| Resolve on unknown tx                        | resolve alone                               | `UnknownTx`             |
| Chargeback on unknown tx                     | chargeback alone                            | `UnknownTx`             |
| Dispute on a withdrawal tx                   | deposit, withdraw, dispute(tx of withdraw)  | `UnknownTx`             |
| Resolve on a withdrawal tx                   | deposit, withdraw, resolve(tx of withdraw)  | `UnknownTx`             |
| Chargeback on a withdrawal tx                | deposit, withdraw, chargeback(tx of withdraw)| `UnknownTx`            |
| Dispute on Disputed                          | deposit, dispute, dispute                   | `AlreadyDisputed`       |
| Dispute on Resolved                          | deposit, dispute, resolve, dispute          | `WrongTxState`          |
| Dispute on ChargedBack                       | deposit, dispute, chargeback, dispute       | `AccountFrozen`*        |
| Resolve on Posted (never disputed)           | deposit, resolve                            | `WrongTxState`          |
| Resolve on Resolved                          | deposit, dispute, resolve, resolve          | `WrongTxState`          |
| Resolve on ChargedBack                       | deposit, dispute, chargeback, resolve       | `AccountFrozen`*        |
| Chargeback on Posted                         | deposit, chargeback                         | `WrongTxState`          |
| Chargeback on Resolved                       | deposit, dispute, resolve, chargeback       | `WrongTxState`          |
| Chargeback on ChargedBack                    | deposit, dispute, chargeback, chargeback    | `AccountFrozen`*        |
| Dispute by mismatched client                 | client A deposits; client B disputes        | `ClientMismatch`        |
| Resolve by mismatched client                 | A deposit, A dispute; B resolve             | `ClientMismatch`        |
| Chargeback by mismatched client              | A deposit, A dispute; B chargeback          | `ClientMismatch`        |

\* Rows referencing a `ChargedBack` deposit hit `AccountFrozen` first because chargeback locks the account. To exercise the `WrongTxState` branch on a terminal-`ChargedBack` deposit *without* frozen-account masking, the test needs a second deposit on the same client *before* the chargeback completes — but chargeback freezes regardless, so post-chargeback rows always hit `AccountFrozen` first. This is correct behaviour and is documented in the test as the reason these three rows expect `AccountFrozen`, not `WrongTxState`. The `Resolved` cases above carry the load for "dispute/resolve/chargeback against terminal state".

### Invariant assertions (helper called from every accepted-op test)

- `account.total() == account.available + account.held`.
- After accepted dispute: `available_after + held_after == available_before + held_before`.
- After accepted chargeback: `held_after == held_before - amount`, `total_after == total_before - amount`, `account.locked == true`.

## Critical files

- `src/lib.rs` — new (module wiring + re-exports of `Tx`, `Account`, `Engine`, `IgnoreReason`).
- `src/domain.rs` — new (types from "Public types" above).
- `src/engine.rs` — new (`Engine`, handlers, `IgnoreReason`, internal `DepositRecord`).
- `tests/engine.rs` — new (matrix above).
- `Cargo.toml` — add `rust_decimal` dep.
- `src/main.rs` — **untouched this milestone** (still Hello World; rewired next milestone).

## Verification

- `cargo build` — compiles cleanly, edition 2024, no warnings.
- `cargo test` — all engine tests in `tests/engine.rs` pass.
- Coverage check: every row in the "Transition coverage" and "Ignored-input coverage" tables maps to at least one test name in the file.
- `total == available + held` invariant helper is called from every accepted-op test.

## Out of scope (deferred to next milestone's plan)

- CSV parsing (`csv_io.rs`, the `csv` crate, whitespace tolerance, 4dp parse rules, strict rejection of `dispute,c,t,5.0`-shaped rows, parse-error stderr surfacing).
- `main.rs` rewiring (argv, exit codes, missing-arg usage).
- Stderr *call site* for `IgnoreReason` (the engine *exposes* the typed reason here; the `eprintln!` in `main` ships next milestone alongside CSV wiring).
- 4dp formatting of `available`/`held`/`total` for output (display concern).
- Integration tests with golden CSV fixtures.
- Performance benchmarks against alternative decimal crates.

## Permanently out of scope (not deferred — never)

- **Output row ordering.** `state-machine.md` declares ordering unspecified; this is a project-level decision, not a deferred chore. Engine and writer both keep `HashMap` iteration order.
