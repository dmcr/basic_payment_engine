# Devlog

This devlog exists for me to give a running narrative of how this project was built.
I will loosely organize this narrative by milestone.
I aim to record engineering decisions, judgement and ai usage for transparency.
All words here are my own.

## Milestone 0 — Setup

### Documentation approach

First thing to decide is how best to record engineering decisions, ai usage and judgement.
Retrospective documentation on development and raw dumping of prompts and sessions is not
particulary insightful. I feel that the most insightful approach is to provide a running 
narrative alongside commits capturing: engineering decisions, artifacts and ai usage etc.
So I have created this document for narration and [`context`](./context/) for artifacts.

Complete ai declaration here: [`AI_USAGE.md`](./AI_USAGE.md) with more details.

I will capture assumptions here: [`ASSUMPTIONS`](./ASSUMPTIONS.md) for ease of lookup.

**AI Usage**:
Validated approach, Generated structure and blank docs.

### Git approach

For time and review simplicity we will commit to main using `chore:`,`feat:`,`test:`, etc.

## Milestone 1 - Domain Model/Rules

I considered I/O first but settled on defining the prerequisites first: the domain types as the I/O and engine depend on the underlying types. **AI Usage**: Discussed options and validated approach.

### Domain model

#### Input/Transactions Type

A csv list of the transactions type:
type=deposit|withdrawal|dispute|resolve|chargeback
client=u16
tx=u32
amount=decimal

**assume** that `type` is a string - expect edge cases
**assume** client is valid u16
**assume** tx is valid u32
**assume** the amount is a decimal up to four places past the dp
**assume** client ids and transaction ids are globall unique
need to choose a decimal library to handle floating point issues

Transaction `type` rules:
deposit: A credit to a cleints asset account - available and total funds should increase
  amount: Whitespaces and decimal precisions up to four plaxes past the decimal must be accepted.
withdrawl: A debit to a clients asset account - available and total funds should decrease **or fail if insufficient funds**
  amount: Whitespaces and decimal precisions up to four plaxes past the decimal must be accepted.
dispute: A clients available funds should decrease by amount disputed and held funds increase by a corresponding amount. **Total funds unchanged.**
  amount=null: Amount is found through transaction id (the disputed transaction) **ignore if not found**
resolve: Dispute resolution, releasing held funds, increasing available funds by corresponding amount. **Total funds unchanged.**
  amount=null: Amount is determined by transaction id like disputes. **ignore if not found OR is not currently disputed**
chargeback: Held and total funds should be decreased by disputed amount **and account frozen**
  amount=null: Amount is determine by transaction id lookup. **If it doesnt exist OR it isnt under dispute then ignore.**

We should work through the rules and look for missing rules/assumptions/edge cases;

Should we allow negative amount values?
Negative values goes against the implications of the system setup which is clearly meant to work with positive amounts.
**assume**/**rule**:
Consider transactions with negative amounts to be malformed and ignore.

Should we allow the balance to ever go negative?
There are edge cases where balance may go negative e.g. a withdrawl has occured but a chargeback causes us to go negative.
In this case I believe we should allow negativity though we will have to ensure our system handles that possibility during maths.
**assumption**/**rule**:
Balances may go negative in response to chargebacks

What do we do with subsequent transactions for frozen accounts?
My instinct says we should ignore all transactions once an account is frozen. We have already decided fraud is occuring on the account.
I believe a bank at this point would freeze all further activity.
**assumption**/**rule**:
Ignore all subsequent transactions for frozen accounts

Can resolved disputes be re-disputed?
Disputed -> chargeback -> frozen = no futher disputes
Disputed -> resolved -> further disputes?
**AI Prompt**:
Do financial institutions allow transactions to be disputed more than once. specifically would a payment engine allow funds to be disputed, held and then
returned after resolution and then be disputed again? My feeling is no otherwise funds could be continually contested and frozen in all but name?
**Outcome**:
Validated instinct. Dispute resolutions carry findings and overturning the resolution would require escalation revisiting the dispute not raising a new one.
**assumption**/**rule**:
A transaction can be disputed only once

Can we dispute withdrawals or just deposits?
My instinct says we should assume that we can only dispute deposits. You cannot hold something that has already been withdrawn.
**AI Prompt**:
My instinct says we only dispute deposits, you cannot hold something that has already been withdrawn. discuss and validate based on existing rules @DEVLOG.md
**outcome**:
Walked through rules scenarios and validated instinct and understanding:
disputes are modelled around deposit fraud scenarios. If we reduced by x on withdrawal, dispute reducing another x and then also chargeback we would remove 3x 
from the client. This isnt something a bank or financial institution would do.
**assumption**/**rule**: 
A dispute can only be placed on a deposit and not a withdrawal

**AI prompt**: Review the rules so far in @DEVLOG.md and determine if I have missed any additional edge cases.

Client mismatches on dispute - dispute row carries a client, must it match for the transaction id?
We should always check if the client id and transaction id match otherwise reject.
**assumptions**/**rule**:
Where the clientid AND transaction id of a dispute do not match the referenced transaction assume error or malformed and ignore the dispute.

#### Account/Output Type

A csv list file of the account/output type:
client=u16
available=decimal
held=decimal
total=decimal
locked=boolean

Notes:
total = available + held (key engine invarient)
available to 4 d.p.
Csv client order does not matter, can display decimals for round values.

#### Decimal

We need something to handle floating point issues.
For production we would want to do benchmarks and a thorough analysis of library.
**AI Prompt**: Discuss performant production ready solutions for decimal crates
Most performant without a dep would be Integer scaling but not suitable here as the tradeoff is bugs and clean code. Out of scope for now.
Use the standard rust_decimal crate. No heap allocation per op so fine for streaming/large inputs. Production ready for similar uses.

### Artifacts

At this point it is time to formalise all this into a domain-model artifact and a state machine artifact. I believe this provides a clean
seperation of what has been determined so far from thinking about the domain model/rules: the static structure and the dynamic behaviour.
**AI Prompt**: 
Given the Domain model from @DEVLOG.md lets draft a formal domain-model.md in @context. It should be small and structural.
Include validation rules. Exclude dynamic behaviour. We will make a seperate form state-machine doc to capture dynamic rules and transitions.
Do not make assumptions, instead interview me for anything you do not know or decision that need to be made.
**Engineering Judgement**:
Include negative amounts in input transactions as a validation rules so that it never enters the system allowing engine to focus on engine
behaviour.
Make a note in the doc about parsing whitespace.
Show me visually walking through each rule that mentions total to confirm that we can derive it for output but not need to store it internally.
Confirmed. This eliminates a class of bugs, ops and reduces storage size. Total is derived.
**Artifact**:
Produced a formal [`domain-model.md`](./context/domain-model.md) through prompts based on my domain model here in the devlog.

Before implementing the types lets also formalise the state machine which formalises the dynamic behaviour and rules of the static model.
**AI Prompt**:
Given the Domain model from @DEVLOG.md we have already formalised the static domain types @context/domain-model.md. We additionally need to
formalise the dynamic bahviour and rules. Lets do this as a state machine doc. Lets draft and iterate on a state machine file in the context
folder. I expect we will surface some decision areas. Bring those to me in the discussion as we iterate so I can exercise my judgement.
**Engineering Judgement**:
Model acount locking (frozen) as a seperate state machine or a per transaction guard condition. Coneptually clearer to model seperate.
Discussion surfaced that client-match assumption (dispute rows client must match referenced transactions client) should apply to resolve and
chargeback rows. I will update the assumptions. This is in keeping with financial institutions who would not allow a transaction to be carried
out by one client on another client.
Should we create a new client if it does not exist in all scenarios? Technically we should only create a new account in deposit situations.
This is because technically it is impossible to perform a withdrawal without an account (no account=0, balance) and the same can be said for
the other transaction types, you actually cant perform a dispute, resolve or chargeback if there is no transaction to reference. There can
only be a transaction to reference if we have had at least one deposit. We may wish to still create the account with 0 balance however so
we can test expected output of that client as 0. So lets create account with balances of 0 unless deposit and ignore transactions under there
own rules in these circumstances.
**Artifact**:
Produced a formal state machine doc for dynamic behaviour model [`state-machine.md](./context/state-machine.md) note this does not dictate we
build a state machine just it is a natural representation to capture all states, transitions and rules.

## Milestone 2 - Engine Types, Data Structures, Functions and Unit Test Plan + Implementation

**AI Prompt**: /plan Produce an implementation plan for the payment engine described in @DEVLOG.md, @ASSUMPTIONS.md, @context/domain-model.md,
@context/state-machine.md. Treat those four documents as the canonical specification. Paritcularly the formal @context documents. The plan should 
cover: project layout (modules), public types, the engines input/output flow, the five row handler functions, error/ignore handling, and a unit-test 
strategy that exercises every transition and ignored input case from the state machine. Do not write code yet. Surface any gap or ambiguity in the 
artifacts before producing the plan. We will make a seperate plan for CSV Parsing, I/O and integrations. We should prioritise clean, working,
extensible, idiomatic rust code over efficiency. That said we should not neglect easy performance wins from a time space complexity. 
That said I believe hashmaps O(1) lookups are the play here for our data structures.
**Surfaced/Decisions/Clarity**
Should we ignore dispute,resolve,chargeback's with amounts. I think we should be strict and consider it malformed and ignore it. We may revisit this.
Internal state data structure. I already decided what structure the 'state machine'/'engine' will take. Just pure functions for each transaction type.
I also decided on a hashmap for looking up disputes. However I did not decide on the 'account' data structure. Output is not required to be sorted
but there are benefits to testing if we can get it for near free. If we can also gain/keep near O(1) time complexity and not overly effect space then
why not. I will have a discussion with claude on the options vs just using my original plan of a hashmap. Decided to stick with initial plan for hashmap.
We could consider using a BTreeMap<u16, Account> instead of HashMap<u16, Account> in the future if we need sorted output. The cost would be O(1) lookups
/upserts > O(log n). If we did sorting on the hashmap then the output sort at the end would be a one time O(n log n) anortised across the whole stream.
Decided to add sorting of hashmap at the end for ease of testing though we might have to revisit that if it is going to cause us issues with large test
files. Review.
Engine to surface errors but not be responsible for printing.
**Artifacts**:
End result of prompting and follow up iterations is the implementation plan for handover to claude for implementation. ['engine-plan.md'](./context/engine-plan.md)

**AI Prompt**:
Implement this plan. Surface issues to me rather than making decision. Provide a detailed end summary. Follow the @context and plan closely.
**AI Generated Code**
**AI Generated test matrix of engine based on the state-machine rules I built**
Verification against specs, plan and code review by myself and claude.

## Milestone 3 - CSV Parser, Integration and Integration Tests Plan + Implementation

**AI Prompt**: /plan Produce an implementation plan for the CSV parser, I/O, Engine Integration and Integration Tests. For context you can refer to
the @context and the engine implementation and the engine tests @src and @tests. The plan should account for stream values through memory as opposed 
to loading the entire data set upfront so that we can handle large files. Surface any gap or ambiguity in the artifacts before producing the plan. W
We should prioritise clean, working, extensible, idiomatic rust code over efficiency. We should consider performance implications. The integration 
tests will have to account for the fact that output client order isnt guranteed.
**Surfaced/Decisions/Clarity**:
Strict vs lenient row parsing. Ignoring malformed rows as per spec. The parser must tolerate whitespace. Reject negative amounts. Reject amount on
dispute/resolve/chargebacks. Structural failures of a row should also cause consistent behaviour (ignore and raise stderr).
For integration tests we will parse expected and actual into respective hashmaps rather than file sort which is flaky.
Parser should reject as malformed > 4 d.p. amounts.
Errors going to stderr should include the raw row data for debugging.
We should pass integration tests from realistic csv files commited not strings.
We will co-locate the reader and writer for now but would revisit if we expanded.
Use Bankers rounding (half to even) for rounding decisions rather than half away from zero.
Do not exit on handled errors/expected errors.
We should not only test happy paths but edge cases according to the prior art on testing.
**Artifacts**:
Generated and iterated on ['csv-engine-integration-plan.md'](./context/csv-engine-integration-plan.md)






