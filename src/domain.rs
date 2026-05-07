pub type Decimal = rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tx {
    Deposit { client: u16, tx: u32, amount: Decimal },
    Withdrawal { client: u16, tx: u32, amount: Decimal },
    Dispute { client: u16, tx: u32 },
    Resolve { client: u16, tx: u32 },
    Chargeback { client: u16, tx: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub client: u16,
    pub available: Decimal,
    pub held: Decimal,
    pub locked: bool,
}

impl Account {
    pub fn zero_for(client: u16) -> Self {
        Self {
            client,
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            locked: false,
        }
    }

    pub fn total(&self) -> Decimal {
        self.available + self.held
    }
}
