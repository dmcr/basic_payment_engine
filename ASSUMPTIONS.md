# Assumptions

- The client has a single asset account. All transactions are to and from this single asset account.
- There are multiple clients. Transactions reference clients. If a client doesn't exist create a new record.
- Client are represented by u16 integers. No names, addresses, or complex client profile info.