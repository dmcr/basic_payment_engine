use std::collections::HashMap;

use crate::domain::{Account, Decimal, Tx};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreReason {
    AccountFrozen,
    InsufficientFunds,
    UnknownTx,
    WrongTxState,
    AlreadyDisputed,
    ClientMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepositState {
    Posted,
    Disputed,
    Resolved,
    ChargedBack,
}

struct DepositRecord {
    client: u16,
    amount: Decimal,
    state: DepositState,
}

#[derive(Default)]
pub struct Engine {
    accounts: HashMap<u16, Account>,
    deposits: HashMap<u32, DepositRecord>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, tx: Tx) -> Result<(), IgnoreReason> {
        match tx {
            Tx::Deposit { client, tx, amount } => self.on_deposit(client, tx, amount),
            Tx::Withdrawal { client, amount, .. } => self.on_withdrawal(client, amount),
            Tx::Dispute { client, tx } => self.on_dispute(client, tx),
            Tx::Resolve { client, tx } => self.on_resolve(client, tx),
            Tx::Chargeback { client, tx } => self.on_chargeback(client, tx),
        }
    }

    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }

    fn on_deposit(&mut self, client: u16, tx: u32, amount: Decimal) -> Result<(), IgnoreReason> {
        let account = self
            .accounts
            .entry(client)
            .or_insert_with(|| Account::zero_for(client));
        if account.locked {
            return Err(IgnoreReason::AccountFrozen);
        }
        account.available += amount;
        self.deposits.insert(
            tx,
            DepositRecord { client, amount, state: DepositState::Posted },
        );
        Ok(())
    }

    fn on_withdrawal(&mut self, client: u16, amount: Decimal) -> Result<(), IgnoreReason> {
        let account = self
            .accounts
            .entry(client)
            .or_insert_with(|| Account::zero_for(client));
        if account.locked {
            return Err(IgnoreReason::AccountFrozen);
        }
        if account.available < amount {
            return Err(IgnoreReason::InsufficientFunds);
        }
        account.available -= amount;
        Ok(())
    }

    fn on_dispute(&mut self, client: u16, tx: u32) -> Result<(), IgnoreReason> {
        let account = self
            .accounts
            .entry(client)
            .or_insert_with(|| Account::zero_for(client));
        if account.locked {
            return Err(IgnoreReason::AccountFrozen);
        }
        let record = self.deposits.get_mut(&tx).ok_or(IgnoreReason::UnknownTx)?;
        if record.client != client {
            return Err(IgnoreReason::ClientMismatch);
        }
        match record.state {
            DepositState::Posted => {}
            DepositState::Disputed => return Err(IgnoreReason::AlreadyDisputed),
            DepositState::Resolved | DepositState::ChargedBack => {
                return Err(IgnoreReason::WrongTxState);
            }
        }
        account.available -= record.amount;
        account.held += record.amount;
        record.state = DepositState::Disputed;
        Ok(())
    }

    fn on_resolve(&mut self, client: u16, tx: u32) -> Result<(), IgnoreReason> {
        let account = self
            .accounts
            .entry(client)
            .or_insert_with(|| Account::zero_for(client));
        if account.locked {
            return Err(IgnoreReason::AccountFrozen);
        }
        let record = self.deposits.get_mut(&tx).ok_or(IgnoreReason::UnknownTx)?;
        if record.client != client {
            return Err(IgnoreReason::ClientMismatch);
        }
        if record.state != DepositState::Disputed {
            return Err(IgnoreReason::WrongTxState);
        }
        account.available += record.amount;
        account.held -= record.amount;
        record.state = DepositState::Resolved;
        Ok(())
    }

    fn on_chargeback(&mut self, client: u16, tx: u32) -> Result<(), IgnoreReason> {
        let account = self
            .accounts
            .entry(client)
            .or_insert_with(|| Account::zero_for(client));
        if account.locked {
            return Err(IgnoreReason::AccountFrozen);
        }
        let record = self.deposits.get_mut(&tx).ok_or(IgnoreReason::UnknownTx)?;
        if record.client != client {
            return Err(IgnoreReason::ClientMismatch);
        }
        if record.state != DepositState::Disputed {
            return Err(IgnoreReason::WrongTxState);
        }
        account.held -= record.amount;
        account.locked = true;
        record.state = DepositState::ChargedBack;
        Ok(())
    }
}
