# Assumptions

This document is for quick assumption reference.
Search [`DEVLOG.md`](./DEVLOG.md) for `assume` or `assumption` to see reasonings.

- The client has a single asset account. All transactions are to and from this single asset account.
- There are multiple clients. Transactions reference clients. If a client doesn't exist create a new record.
- Client are represented by u16 integers. No names, addresses, or complex client profile info.
- Input `type` is a string.
- Client is valid u16.
- tx is valid u32.
- The amount is a decimal up to four places past the dp.
- Cleint and Transaction ids are globally unqiue
- Transactions oocur chronologically in input.
- That transactions with negative amounts are malformed and should be ignored.
- Balances may go negative in response to chargebacks
- That all transactions should be ignored after an account has been frozen.
- A transaction can be disputed only once
- Only deposits can be disputed. Assume withdrawal cannot be disputed and discard disputes for withdrawals.
- Where the clientid AND transaction id of a dispute do not match the referenced transaction assume error or malformed and ignore the dispute.
