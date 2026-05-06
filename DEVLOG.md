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

## Next

Current thoughts:
I ended up modelling rules along with the domain model, we should model State machine out the rules/transitions discussed in the domain model
Engine logic should be pure functions so easy to unit test.
Need to build the I/O layer
Decide engine or I/O first
CSV ingestion should be able to handle extreme size
Need to create test input files and expected outputs that encodes all edge case
Perhaps one file that tests all edge cases by specific clients ids allowing us to test expected
outputs fairly easily.
Maybe a script to generate an extremely large input file to validate that it can handle.



