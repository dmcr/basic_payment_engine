# basic_payment_engine

A simple toy payments engine that streams transactions from a CSV, applies them to per client accounts with
support for deposit/withdrawl/dispute/resolve/chargeback support, writes final account balances as CSV to stdout.

## Usage

```sh
cargo run -- transactions.csv > accounts.csv
```

### Testing

```sh
cargo test                      # all tests
cargo test --test engine        # engine unit tests (state-machine matrix)
cargo test --test integration   # I/O integration tests (fixture-driven)
cargo test --lib                # csv_io parser/formatter unit tests
```

Sample data lives in [`tests/fixtures/`](./tests/fixtures/) as paired `<name>.csv` / `<name>.expected.csv` files. Fixtures cover happy path, dispute lifecycle, chargeback freeze, multi-client, whitespace + precision, and malformed rows. The integration tests parse both actual and expected output into a `HashMap<u16, Row>` so row order is not asserted.

To see a fixture against the real binary, redirect stdout (the accounts CSV) and stderr (the ignored row reasons) to tmp files:

```sh
cargo run -- tests/fixtures/malformed_rows.csv > /tmp/out.csv 2> /tmp/err.log
diff /tmp/out.csv tests/fixtures/malformed_rows.expected.csv  # row-order sensitive — for a true match use the integration tests
cat /tmp/err.log                                              # one line per ignored row, with reason and raw row
```

## Implementation details, verification and testing

### Assumptions
See [`ASSUMPTIONS.md`](./ASSUMPTIONS.md) for assumptions

### Implementation choices

Refined transaction types depending on the parse-engine boundary:
Malformed rules happen at parsing so the Engine types are refined meaning the engine can deal purely with transition rules.

Decimal type:
Eliminate floating point issues

Derived total:
We store only avaialble + held internally and derive the total when outputting eliminate a whole class of bugs

Stderr debugging output:
All handled/expected edge cases that lead to 'ignored' input/transaction whether thats invalid at the parse boundary or
invalid due to state machine transition rules are viewable for dev and future debugging with reason and raw row data.

State machine model:
Defining the state machine, the transition rules, gives allot of clarity into allowed states and transitions giving 
confidence in our ability to test and what we should test

Unit + Integration tests:
Engine level unit tests according to the state machines pure functions (onTransaction functions)
where we can test the transition rules based on current state and transaction input in isolation.
I/O level integration tests with csv files testing both the I/O layer (parsing, reading, writing)
and the engine.

### Efficiency
One row buffer end to end, memory is O(clients + deposits)(unqiue) not filesize. That is to say streamed in per row rather than whole file read.
Engine state time complexity using hashmaps internally has O(1) lookups. Possible as we did not need sorting on output.
The engine entry point allows for the CLI binary, integration tests to be shared and theoretically for a one engine per stream to support
thousands of tcp connections on the server. Could achieve through spawning a task per connection with tokio. The engine code would not change
but we could become massively parralel. Well it would likely need a small async refactor of run.


### Safety and Error handlig
Engine layer is somewhat pure and decoupled with clean seperation of concerns. There is no I/O. One error channel.
Parser rejects malformed errors from reaching engine and sends to stderr. Does not interupt operations.
Engine rejected transactions are surfaced to stderr and processing continues.
Both of those error handling situations are spec beahviour not failure modes.
Arg, file-open, IO can still cause exits. The engine should not panic or fail in normal flows.
All built off of a well defined modelled engine for insights into safety and error handling paths.

### Reflections

I spent allot of time up front working through the data model. While doing so I also ended up working through the rules.
This process nessesitated understanding the existing rules and then thinking about the edge cases, missing assumptions.
Given what was to be built it was clear to me pretty early on there are two conceptual components to the solution.

We have the CSV I/O layer + the payment engine itself. There were a fair few assumptions, rules, states and transitions.
I focused on clarifying these knowing that from them we could then model a state machine to verify, conceptualize and
visualize. In practice the implementation did not have to be complex. We could model it as a simple set of pure-ish
functions (one for each transaction type). The advantage of pure-ish functions here is that they are easy to unit test.

So in summary for the engine, after building the context by working through the information we had and then filling in
the blanks, we could then confidently model both the static model and the dynamic model as a state machine. As we could
map these events cleanly to pure-ish functions we knew we could test the engines basic state machine according to the 
docs with unit test coverage. This allowed us to build the payment engine in isolation and seperate the concerns of CSV
I/O from the engine and testing of the engines internals. 

Once we could then move onto the csv-io implementation. Here
integration tests where what we wanted, testing from the same conceptual matrix but black-box, csv in, csv out with the
addition of covering integration of the writer/reader and the engine. In all cases, unit and integration tests, I can 
validate that the implementation is correct according to my specs and assumptions. There are edge cases around file size 
that I have not tested the limits of but steps where taken in the implementation to allow for this.

## Important Development docs for futher reading

See [`DEVLOG.md`](./DEVLOG.md) for a running narrative of design decisions, tradeoffs, and AI tool usage.
See [`AI_USAGE.md`](./AI_USAGE.md) for ai usage declaration
See [`ASSUMPTIONS.md`](./ASSUMPTIONS.md) for assumptions
See [`context`](./context/) for artifacts and raw implementation prompts (plans)