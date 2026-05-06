# basic_payment_engine

A simple toy payments engine that reads a series of transactions from a CSV, updates client accounts, handles disputes and chargebacks, and then outputs the state of clients accounts as a CSV.

Intended as a coding challenge in Rust to gather insights into my engineering process and abilities.

## Usage

```sh
cargo run -- transactions.csv > accounts.csv
```

## Development notes

See [`DEVLOG.md`](./DEVLOG.md) for a running narrative of design decisions, tradeoffs, and AI tool usage.
See [`AI_USAGE.md`](./AI_USAGE.md) for ai usage declaration
See [`ASSUMPTIONS.md`](./ASSUMPTIONS.md) for assumptions
See [`context`](./context/) for artifacts and raw implementation prompts (plans)