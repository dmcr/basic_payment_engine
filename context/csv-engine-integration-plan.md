# Implementation Plan — CSV I/O, Engine Integration, Integration Tests (Milestone 3)

## Context

Milestone 2 shipped the pure engine: `Engine::apply(Tx) -> Result<(), IgnoreReason>` consuming validated `Tx` values, with a HashMap-backed account store and full unit-test coverage of the FSM. `main.rs` is still Hello-World; there is no CSV reader, no writer, no end-to-end wiring.

This milestone closes that gap. Goals:

- Parse the input CSV as a **stream** (one row resident at a time) so multi-GB inputs do not blow memory.
- Validate each row against the spec at the parse boundary; emit well-formed `Tx` values into the engine.
- Wire `main.rs`: argv → file → reader → engine → writer → stdout.
- Surface every ignored row (parser-rejected or engine-rejected) on stderr in a uniform format.
- Add integration tests that drive the binary end-to-end via on-disk fixtures and assert output without depending on row order.

The engine itself is **not changed**. Its public surface is sufficient.

## Decisions (locked at plan time)

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Streaming via `csv::Reader::deserialize()` over `BufReader<File>`. No `collect()`, no full-file load. | One-row buffer; `csv` reuses internal `StringRecord` allocations. |
| D2 | Parser errors → ignore the row + one stderr line. Continue processing. | Matches engine ignore behaviour; keeps partial output usable on dirty inputs. |
| D3 | Over-precision amounts (>4dp) → ignore as malformed. | Spec wording is "at most four decimal places". Rejecting avoids silent rounding. |
| D4 | `amount` present on Dispute/Resolve/Chargeback → ignore as malformed. | `domain-model.md` says amount is absent on lifecycle rows. |
| D5 | Stderr format: `ignored row {N}: {reason} [{raw row}]`. `N` is 1-indexed and excludes the header. | Row number locates the line; raw row aids debugging. |
| D6 | Fixtures on disk under `tests/fixtures/` as paired `<name>.csv` + `<name>.expected.csv`. | Golden-file style; readable diffs; one source of truth per scenario. |
| D7 | Integration tests parse both actual and expected output into `HashMap<u16, Row>` and assert per-client. **No sorting.** | Honours the project rule that output ordering is permanently unspecified; never introduces sort logic. |
| D8 | Module layout: single `src/csv_io.rs`. | Matches the engine-plan layout; reader and writer are small enough that a split is premature. |
| D9 | Writer rounds to 4dp using banker's rounding (`RoundingStrategy::MidpointNearestEven`). | Statistically neutral; financial-standard rounding. |
| D10 | CLI: exactly one positional arg. Missing/extra → usage to stderr, exit 1. File open failure → error to stderr, exit 1. | Matches README. No `clap` dependency. |
| D11 | Exit 0 whenever the file is fully processed, even if rows were ignored. | Ignored rows are documented spec behaviour, not a run failure. |
| D12 | `csv` crate config: `has_headers(true)`, `trim(Trim::All)`, `flexible(false)`. | Trim handles whitespace tolerance; strict column count rejects malformed shapes. |

## Spec gaps surfaced and resolved

- **Engine state on parser-ignored rows.** Resolved: rows the parser rejects never reach `Engine::apply`, so neither balances nor account creation occur for malformed rows. This is consistent with `state-machine.md`'s "Account creation … on the first **well-formed** row".
- **Row counting for stderr.** Resolved: 1-indexed, header excluded — i.e. "row 1" is the first data row a human reading the file expects.
- **Reader buffering size.** Resolved: rely on `BufReader::new(File)` defaults (8 KiB). Sufficient; not worth tuning without a benchmark.
- **Writer flushing.** Resolved: `csv::Writer` will be dropped at end of `main` which flushes; we additionally call `.flush()?` explicitly before exiting to surface I/O errors.
- **Decimal serialisation surface.** Resolved: do not rely on `serde` derive for `Decimal` on the *output* path — format manually via `round_dp(4)` + `Display` to guarantee the `.0000` form. Input path uses `Decimal::from_str_exact` via a custom field deserialiser to enforce the over-precision rule.

## Project layout (this milestone)

```
src/
  lib.rs            +  re-export csv_io::run (or equivalent entry point)
  domain.rs         (unchanged)
  engine.rs         (unchanged)
  csv_io.rs         NEW — InputRow, parsing, Tx construction, writer, run()
  main.rs           rewired — argv parsing, calls csv_io::run, exit codes
tests/
  engine.rs         (unchanged — pure-engine matrix from M2)
  integration.rs    NEW — drives the binary end-to-end on fixtures
  fixtures/
    happy_path.csv
    happy_path.expected.csv
    dispute_lifecycle.csv
    dispute_lifecycle.expected.csv
    chargeback_freezes.csv
    chargeback_freezes.expected.csv
    malformed_rows.csv
    malformed_rows.expected.csv
    whitespace_and_precision.csv
    whitespace_and_precision.expected.csv
    multi_client.csv
    multi_client.expected.csv
```

`Cargo.toml` adds: `csv = "1"`, `serde = { version = "1", features = ["derive"] }`. `rust_decimal` already present.

`[dev-dependencies]`: `assert_cmd = "2"` (drive the binary), `tempfile = "3"` (write transient inputs if needed). Both are standard CLI-test tooling.

## CSV input model

Two layers: a raw deserialisation type (what the CSV actually looks like on the wire) and the engine-facing `Tx` (already exists in `domain.rs`).

```rust
// csv_io.rs

#[derive(Debug, serde::Deserialize)]
struct InputRow {
    #[serde(rename = "type")]
    kind: String,         // parsed downstream; keeps the row deserialisable even if kind is gibberish
    client: u16,
    tx: u32,
    amount: Option<String>, // String, not Decimal — we control parsing to enforce 4dp + non-negative
}
```

Why `String` for amount, not `Option<Decimal>`:

- We need to enforce "at most 4 decimal places" *at parse time*. `Decimal::from_str_exact` does not enforce a precision cap, so we inspect the string before converting.
- It keeps `serde` from short-circuiting the row on a parse error we want to handle as "ignore + stderr".

### Parsing pipeline (per row)

1. `csv::Reader::deserialize::<InputRow>()` yields `Result<InputRow, csv::Error>`.
2. On `Err`: structural malformation (wrong column count, unparsable `u16`/`u32`, etc.). Emit stderr ignored line, continue.
3. On `Ok(row)`: validate semantically via `try_from_input_row(row)`:
   - `kind` parses to a `TxKind` (case-sensitive lower per spec); else ignore.
   - For Deposit/Withdrawal: `amount` is `Some(s)`, `s` parses to a `Decimal`, decimal-place count `<= 4`, value `>= 0`; else ignore.
   - For Dispute/Resolve/Chargeback: `amount` is `None` or empty; else ignore (D4).
4. On valid `Tx`: call `engine.apply(tx)`. If `Err(reason)`, emit stderr ignored line.

The pipeline is a single iteration; nothing is buffered.

```rust
pub fn run<R: Read, W: Write>(input: R, output: W, mut err: impl Write) -> io::Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .flexible(false)
        .from_reader(BufReader::new(input));

    let mut engine = Engine::new();

    for (i, result) in reader.deserialize::<InputRow>().enumerate() {
        let row_number = i + 1; // 1-indexed, header excluded
        match result {
            Err(e) => writeln!(err, "ignored row {row_number}: parse error ({e})")?,
            Ok(row) => match try_into_tx(&row) {
                Err(reason) => writeln!(err, "ignored row {row_number}: {reason} [{row:?}]")?,
                Ok(tx) => {
                    if let Err(reason) = engine.apply(tx) {
                        writeln!(err, "ignored row {row_number}: {reason:?} [{row:?}]")?;
                    }
                }
            },
        }
    }

    write_accounts(BufWriter::new(output), engine.accounts())
}
```

(Signature is illustrative — final names land in implementation.)

### Validation helper

```rust
enum InputReject {
    UnknownKind,
    AmountMissing,
    AmountUnexpected,
    AmountNegative,
    AmountTooPrecise,
    AmountUnparsable,
}
// Display impl gives the stderr-friendly string.

fn try_into_tx(row: &InputRow) -> Result<Tx, InputReject> { ... }
```

`InputReject` is parser-layer only; it never crosses the engine boundary. The engine still owns `IgnoreReason`. Both render via `Display` for stderr.

Note on the over-precision check: scan the substring after the decimal point and count digits. `Decimal::from_str_exact` does **not** itself reject `"1.23456"`. Trim is already applied at the CSV layer.

## CSV output model

```rust
// csv_io.rs

#[derive(Debug, serde::Serialize)]
struct OutputRow {
    client:    u16,
    available: String, // pre-formatted to 4dp
    held:      String,
    total:     String,
    locked:    bool,
}

fn write_accounts<W: Write>(w: W, accounts: impl Iterator<Item = &Account>) -> io::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    for a in accounts {
        wtr.serialize(OutputRow {
            client:    a.client,
            available: format_4dp(a.available),
            held:      format_4dp(a.held),
            total:     format_4dp(a.total()),
            locked:    a.locked,
        })?;
    }
    wtr.flush()
}

fn format_4dp(d: Decimal) -> String {
    d.round_dp_with_strategy(4, RoundingStrategy::MidpointNearestEven).to_string()
    // Pad with trailing zeros if rust_decimal drops them; verify in implementation.
}
```

Output is emitted in `engine.accounts()` iteration order — i.e. HashMap order. **No sort, ever** (locked project rule).

If `to_string()` for an integer-valued `Decimal` returns `"10"` rather than `"10.0000"`, the formatter will explicitly pad. This is verified in implementation; if `rust_decimal` already pads via `round_dp` we leave it alone.

## main.rs rewiring

```rust
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let path = match args.as_slice() {
        [_, p] => p,
        _ => {
            eprintln!("Usage: {} <transactions.csv>", args.first().map(String::as_str).unwrap_or("payment-engine"));
            return std::process::ExitCode::from(1);
        }
    };

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: cannot open {path}: {e}");
            return std::process::ExitCode::from(1);
        }
    };

    if let Err(e) = basic_payment_engine::csv_io::run(file, std::io::stdout(), std::io::stderr()) {
        eprintln!("error: {e}");
        return std::process::ExitCode::from(1);
    }
    std::process::ExitCode::SUCCESS
}
```

`run` accepting injectable `R/W/E` is what makes integration tests trivial without spawning the binary, and what enables a future SDK-style consumer.

## Integration test strategy

Two layers of integration tests:

### Layer A — `csv_io::run` directly (in-process)

Fast, deterministic, no subprocess. Each test:

1. Reads a fixture pair: `tests/fixtures/<name>.csv` and `tests/fixtures/<name>.expected.csv`.
2. Calls `run(file, &mut Vec<u8>, &mut Vec<u8>)`.
3. Parses *both* `actual_stdout` and `expected.csv` into `HashMap<u16, OutputRow>` via the same `csv::Reader` config.
4. Asserts: same key set, and per-client field equality (with `Decimal` parsed back from string for tolerant comparison if desired — but exact string equality is fine since both sides use the same formatter).

```rust
fn parse_output(bytes: &[u8]) -> HashMap<u16, ExpectedRow> {
    csv::Reader::from_reader(bytes)
        .deserialize::<ExpectedRow>()
        .map(|r| r.unwrap())
        .map(|r| (r.client, r))
        .collect()
}

fn assert_outputs_match(actual: &[u8], expected: &[u8]) {
    let a = parse_output(actual);
    let e = parse_output(expected);
    assert_eq!(a.keys().collect::<HashSet<_>>(), e.keys().collect::<HashSet<_>>(), "client set mismatch");
    for (k, ev) in &e {
        assert_eq!(a.get(k), Some(ev), "row mismatch for client {k}");
    }
}
```

### Layer B — binary end-to-end (one smoke test)

Single test using `assert_cmd` to invoke the compiled binary against `tests/fixtures/happy_path.csv`, redirecting stdout, and using the same `assert_outputs_match` helper. Confirms argv parsing, file open, exit code = 0.

Two negative tests with `assert_cmd`:
- No args → exit 1, stderr contains `Usage:`.
- Nonexistent file → exit 1, stderr contains `cannot open`.

### Fixtures (six pairs)

| Fixture | Covers |
|---------|--------|
| `happy_path` | Multiple clients, deposits + withdrawals only. Confirms basic accounting and 4dp formatting. |
| `dispute_lifecycle` | Deposit → dispute → resolve. Total preserved; account not locked. |
| `chargeback_freezes` | Deposit → dispute → chargeback. Account locked; subsequent rows for that client ignored. |
| `multi_client` | Interleaved rows across 3+ clients, including a client whose only rows fail engine guards (account exists with zeros). |
| `whitespace_and_precision` | Rows with leading/trailing spaces, amounts at exactly 4dp, integer amounts (`5` vs `5.0`). Confirms `Trim::All` and `from_str_exact`. |
| `malformed_rows` | One of each rejection class: bad `type`, missing amount on deposit, amount on dispute, amount with 5dp, negative amount, wrong column count. Expected output ignores all of them; only the well-formed companion rows shape the output. |

Each `.csv` is a small handcrafted scenario; `.expected.csv` is its predicted output. Output ordering is **not** asserted — the parse-into-HashMap helper sees to that.

### Stderr assertions

Layer A tests can optionally assert that the stderr buffer is non-empty and contains the expected reason substring (`InsufficientFunds`, `parse error`, etc.) for the malformed/ignored fixtures. We will assert *substring* presence, not exact format, to keep the format string tweakable without test churn.

## Performance posture

- **Streaming guarantee**: one-row buffer end to end. The reader never collects, the engine retains only the deposit set + account map, the writer iterates the engine and flushes per row.
- **Buffered I/O**: `BufReader<File>` for input, `BufWriter` for output. `csv::Writer` adds its own buffering on top.
- **No allocations per row beyond what `csv` and `Decimal` require.** `InputRow.amount: Option<String>` does allocate per row — acceptable given the alternative (custom `Visitor`) is a noticeable complexity tax for marginal gain. Worth a comment in the code.
- **No parallelism, no `mmap`, no SIMD.** Single-threaded reader/engine/writer pipeline. The engine is the throughput floor and is `O(1)` per row.
- **HashMap default hasher** (SipHash) is fine. We are not hash-DoS-sensitive here, but switching to `FxHash` is a one-liner if benchmarks ever motivate it.

Big-O recap: time `O(n)` in input rows; space `O(c + d)` where `c` = unique clients, `d` = unique deposit txs. Independent of input file size beyond those two.

## Critical files

| File | Action |
|------|--------|
| `src/csv_io.rs` | NEW — `InputRow`, `OutputRow`, `InputReject`, `try_into_tx`, `run`, `write_accounts`, `format_4dp`. |
| `src/lib.rs` | EDIT — `pub mod csv_io;` and `pub use csv_io::run;` (or similar). |
| `src/main.rs` | REWRITE — argv parsing, file open, call `csv_io::run`, exit codes. |
| `Cargo.toml` | EDIT — add `csv`, `serde` (with `derive`); add `[dev-dependencies]` `assert_cmd`, `tempfile`. |
| `tests/integration.rs` | NEW — Layer A and Layer B tests, fixture loader, `assert_outputs_match` helper. |
| `tests/fixtures/*.csv` | NEW — six input/expected pairs. |
| `src/engine.rs` | UNTOUCHED. |
| `src/domain.rs` | UNTOUCHED. |
| `tests/engine.rs` | UNTOUCHED. |

## Verification

1. `cargo build` — clean build, no warnings, edition 2024.
2. `cargo test` — all of:
   - existing `tests/engine.rs` (M2 matrix) still green;
   - `tests/integration.rs` Layer A passes against all six fixture pairs;
   - Layer B binary smoke test passes;
   - negative CLI tests (`no args`, `bad path`) pass.
3. Manual smoke: `cargo run -- tests/fixtures/happy_path.csv > /tmp/out.csv 2> /tmp/err.log` and diff `/tmp/out.csv` against `happy_path.expected.csv` after parsing both into the comparison helper. Inspect `/tmp/err.log` to confirm stderr lines for malformed-row fixture.
4. Streaming sanity: feed a synthetic 10M-row file (generated locally, not committed) through the binary. Process RSS should stay bounded by `O(unique clients + unique deposits)`, not by file size. This is a one-off check, not a committed test.

## Out of scope (this milestone)

- Sorting output rows. **Permanently** out of scope per project rule.
- Async I/O, tokio, parallel readers.
- Property-based tests (`proptest`/`quickcheck`) — viable but not worth the dependency for the scope here.
- Benchmark harness (`criterion`).
- A `--strict` mode that exits non-zero on ignored rows.
- Reading from stdin (single positional arg only).
- Internationalised number formats (we accept `.` as the decimal separator only, per spec).

## Risk register

- **`Decimal::to_string()` may not pad to 4dp.** Mitigation: `format_4dp` helper, verified in unit test (asserts `"10"` Decimal renders as `"10.0000"`). If `round_dp` strips trailing zeros, switch to `format!("{:.4}", d)` after rounding — verify which `Decimal` impl is in play.
- **`csv::Reader::deserialize` error semantics.** A `csv::Error` mid-stream might be a transient `io::Error` (disk read fail) rather than a malformed row. Mitigation: inspect `e.kind()` — `ErrorKind::Io` aborts the run with a non-zero exit; field/parse errors continue. Document in code.
- **`Trim::All` interaction with empty `amount` field.** A trimmed empty string deserialises as `Some("")` not `None`. Mitigation: `try_into_tx` treats empty-string amount as absent for lifecycle rows; for deposit/withdrawal it routes through `AmountMissing`. Cover in test.
