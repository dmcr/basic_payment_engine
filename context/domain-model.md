# Domain Model

Static structure of the payment engine: types, fields, invariants, and the
mapping between CSV rows and in-memory types. Dynamic behaviour — transition
rules, dispute lifecycle, guard conditions — lives in `state-machine.md`.

## Input — Transaction row

One row of the input CSV.

| Column | Rust type           | Notes                                                                                  |
|--------|---------------------|----------------------------------------------------------------------------------------|
| type   | `TxKind`            | One of: `deposit`, `withdrawal`, `dispute`, `resolve`, `chargeback`.                   |
| client | `u16`               | Client account identifier.                                                             |
| tx     | `u32`               | Globally unique transaction identifier.                                                |
| amount | `Option<Decimal>`   | Required for `deposit` / `withdrawal`. Absent for `dispute` / `resolve` / `chargeback`.|

### TxKind

```rust
enum TxKind {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}
```

### Input validation

A row is **well-formed** if and only if:

- `type` parses to a known `TxKind`.
- `client` is a valid `u16`.
- `tx` is a valid `u32`.
- `amount` is present for `Deposit` and `Withdrawal`, has at most four decimal
  places, and is **non-negative** (`amount >= 0`).
- `amount` is absent (or empty) for `Dispute`, `Resolve`, and `Chargeback`.

Malformed rows — including rows with negative amounts — are ignored. The CSV
parser tolerates whitespace inside fields.

## Account — engine state per client

```rust
struct Account {
    client: u16,
    available: Decimal,
    held: Decimal,
    locked: bool,
}
```

### Invariant

`total == available + held` must hold after every accepted transaction.

`total` is not stored on the `Account`; it is computed at output time as
`available + held`. Treating it as a derived value rather than a stored field
removes a class of inconsistency where `total` could drift from the sum of its
parts.

Both `available` and `held` may be negative — see `ASSUMPTIONS.md` for the
chargeback-after-withdrawal case.

## Output — Account row

One row of the output CSV per known client.

| Column    | Rust type | Notes                                                       |
|-----------|-----------|-------------------------------------------------------------|
| client    | `u16`     | `Account.client`.                                            |
| available | `Decimal` | `Account.available`, formatted to four decimal places.       |
| held      | `Decimal` | `Account.held`, formatted to four decimal places.            |
| total     | `Decimal` | Computed as `available + held`, formatted to four decimal places. |
| locked    | `bool`    | `Account.locked`.                                            |

Row ordering is unspecified. All decimal columns are emitted to four places.

## Decimal type

`rust_decimal::Decimal` is the canonical money type throughout the system,
used for `amount` on input rows and for `available` / `held` / `total` on
accounts. Rationale in `DEVLOG.md`.

## Out of scope (owned by `state-machine.md`)

- Dispute lifecycle and per-transaction state.
- Which input rows cause which balance changes, under which guards.
- Which transactions are retained for later lookup, since this depends on the
  dispute lifecycle.
