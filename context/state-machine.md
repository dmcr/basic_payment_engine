# State Machine

Dynamic behaviour of the payment engine. Two parallel state machines govern
the system at different scopes:

- **Account** — one per client; tracks lock state.
- **Transaction** — one per deposit; tracks dispute lifecycle.

The two interact through (a) a cross-machine trigger fired on chargeback,
and (b) the Account state acting as a guard on every Transaction transition.

For static structure (types, fields, invariants), see `domain-model.md`.
For the underlying derived rules, see `ASSUMPTIONS.md`.

## Account state machine

Scope: one instance per client.

```
   ┌──────────┐
   │  Active  │  initial
   └────┬─────┘
        │ chargeback fires on any of this account's deposits
        ▼
   ┌──────────┐
   │  Frozen  │  terminal
   └──────────┘
```

| From   | Trigger                                | Guard                                  | To     | Effect                  |
|--------|----------------------------------------|----------------------------------------|--------|-------------------------|
| Active | A deposit transitions to `ChargedBack` | (already gated by per-tx guards)       | Frozen | `account.locked = true` |

`Frozen` has no outgoing transitions. While in `Frozen`, all input rows
referencing this client are ignored.

## Transaction state machine

Scope: one instance per **deposit** transaction. Withdrawals are not
subjects of this machine and have no lifecycle state.

```
   ╔═══════╗  deposit row     ┌────────┐    dispute    ┌──────────┐
   ║ (n/a) ║ ───────────────▶ │ Posted │ ────────────▶ │ Disputed │
   ╚═══════╝                  └────────┘               └────┬─────┘
                                                            │
                                                  resolve   │   chargeback
                                                            ▼
                                                  ┌──────────┐ ┌─────────────┐
                                                  │ Resolved │ │ ChargedBack │
                                                  │(terminal)│ │ (terminal)  │
                                                  └──────────┘ └──────┬──────┘
                                                                      │ also fires
                                                                      │ Account → Frozen
                                                                      ▼
```

| From     | Trigger        | Guards                                                                                       | To          | Balance effect                                   |
|----------|----------------|----------------------------------------------------------------------------------------------|-------------|--------------------------------------------------|
| (n/a)    | deposit row    | row is well-formed; account is not `Frozen`                                                  | Posted      | `available += amount`                            |
| Posted   | dispute row    | row is well-formed; account is not `Frozen`; row's `client` matches deposit's client         | Disputed    | `available -= amount`, `held += amount`          |
| Disputed | resolve row    | row is well-formed; account is not `Frozen`; row's `client` matches deposit's client         | Resolved    | `available += amount`, `held -= amount`          |
| Disputed | chargeback row | row is well-formed; account is not `Frozen`; row's `client` matches deposit's client         | ChargedBack | `held -= amount`; also triggers Account → Frozen |

`Resolved` and `ChargedBack` are terminal. Any subsequent dispute / resolve /
chargeback row referencing a transaction in a terminal state is ignored.

The client-match guard applies symmetrically to dispute, resolve, and
chargeback rows: a partner cannot lifecycle another client's dispute.

## Non-stateful row: withdrawal

Withdrawals affect account balance but do not create a Transaction record
and have no lifecycle state.

| Trigger        | Guards                                                                  | Effect                |
|----------------|-------------------------------------------------------------------------|-----------------------|
| withdrawal row | row is well-formed; account is not `Frozen`; `account.available >= amount` | `available -= amount` |

A withdrawal that fails the sufficient-funds guard is ignored; balances are
unchanged.

## Account creation

An `Account` enters the system, in `Active` state with zero balances, on the
**first well-formed row that references its client**, regardless of whether
the row's domain guards subsequently pass. Malformed rows (failing the input
validation rules in `domain-model.md`) do not create accounts.

Consequences:

- A client whose only well-formed rows fail their domain guards (for example,
  a single withdrawal against an empty account, or a dispute referencing a
  non-existent transaction) still appears in the output row with
  `available = 0`, `held = 0`, `total = 0`, `locked = false`.
- A client mentioned only on malformed rows produces no `Account` and no
  output row.

## Ignored input

Any row that fails any of the conditions below is ignored without effect on
account state, balances, or per-transaction state:

- Validation rules in `domain-model.md` (malformed type, malformed amount,
  negative amount, etc.).
- Account is `Frozen`.
- For withdrawal: `account.available < amount`.
- For dispute / resolve / chargeback: referenced `tx` does not exist, refers
  to a non-deposit, is in a terminal per-transaction state, or the row's
  `client` does not match the referenced deposit's `client`.
- For resolve / chargeback specifically: referenced transaction is not in
  `Disputed`.

## Storage

Only **deposit** transactions are retained for later lookup, each holding its
amount, client, and dispute state (`Posted`, `Disputed`, `Resolved`,
`ChargedBack`). Withdrawals are applied to the balance and discarded.
Dispute, resolve, and chargeback rows are processed in place as state
transitions and are never retained.

## Out of scope

- Type definitions, field shapes, and the `total = available + held`
  invariant (`domain-model.md`).
- CSV parsing details and input validation (`domain-model.md`).
- Concrete engine implementation (data structures, function signatures, error
  types, persistence beyond the storage shape stated above).
