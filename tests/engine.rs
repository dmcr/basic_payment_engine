use basic_payment_engine::{Account, Decimal, Engine, IgnoreReason, Tx};

fn dec(s: &str) -> Decimal {
    Decimal::from_str_exact(s).expect("valid decimal in test")
}

fn account_of(engine: &Engine, client: u16) -> &Account {
    engine
        .accounts()
        .find(|a| a.client == client)
        .expect("account exists")
}

fn check_total(account: &Account) {
    assert_eq!(account.total(), account.available + account.held);
}

// ─── Transition coverage ──────────────────────────────────────────────────────

#[test]
fn t1_deposit_creates_account_and_credits_available() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();

    let a = account_of(&e, 1);
    assert_eq!(a.available, dec("10.0"));
    assert_eq!(a.held, Decimal::ZERO);
    assert!(!a.locked);
    check_total(a);
}

#[test]
fn t2_dispute_moves_available_to_held() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    let total_before = account_of(&e, 1).total();

    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();

    let a = account_of(&e, 1);
    assert_eq!(a.available, Decimal::ZERO);
    assert_eq!(a.held, dec("10.0"));
    assert_eq!(a.total(), total_before);
    assert!(!a.locked);
    check_total(a);
}

#[test]
fn t3_resolve_returns_held_to_available() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();
    let total_before = account_of(&e, 1).total();

    e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap();

    let a = account_of(&e, 1);
    assert_eq!(a.available, dec("10.0"));
    assert_eq!(a.held, Decimal::ZERO);
    assert_eq!(a.total(), total_before);
    assert!(!a.locked);
    check_total(a);
}

#[test]
fn t4_chargeback_decreases_held_and_total() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();
    let held_before = account_of(&e, 1).held;
    let total_before = account_of(&e, 1).total();

    e.apply(Tx::Chargeback { client: 1, tx: 100 }).unwrap();

    let a = account_of(&e, 1);
    assert_eq!(a.held, held_before - dec("10.0"));
    assert_eq!(a.total(), total_before - dec("10.0"));
    check_total(a);
}

#[test]
fn t5_account_locked_after_chargeback() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();
    e.apply(Tx::Chargeback { client: 1, tx: 100 }).unwrap();

    assert!(account_of(&e, 1).locked);
}

#[test]
fn withdrawal_decreases_available() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Withdrawal { client: 1, tx: 101, amount: dec("3.0") }).unwrap();

    let a = account_of(&e, 1);
    assert_eq!(a.available, dec("7.0"));
    assert_eq!(a.held, Decimal::ZERO);
    check_total(a);
}

#[test]
fn account_creation_via_failing_dispute_yields_zero_account() {
    let mut e = Engine::new();
    let err = e.apply(Tx::Dispute { client: 7, tx: 999 }).unwrap_err();
    assert_eq!(err, IgnoreReason::UnknownTx);

    let a = account_of(&e, 7);
    assert_eq!(a.available, Decimal::ZERO);
    assert_eq!(a.held, Decimal::ZERO);
    assert!(!a.locked);
}

// ─── Ignored input: Frozen account ────────────────────────────────────────────

fn freeze_account(e: &mut Engine, client: u16, tx: u32, amount: &str) {
    e.apply(Tx::Deposit { client, tx, amount: dec(amount) }).unwrap();
    e.apply(Tx::Dispute { client, tx }).unwrap();
    e.apply(Tx::Chargeback { client, tx }).unwrap();
    assert!(account_of(e, client).locked);
}

#[test]
fn frozen_ignores_deposit() {
    let mut e = Engine::new();
    freeze_account(&mut e, 1, 100, "10.0");

    let err = e
        .apply(Tx::Deposit { client: 1, tx: 200, amount: dec("5.0") })
        .unwrap_err();
    assert_eq!(err, IgnoreReason::AccountFrozen);
}

#[test]
fn frozen_ignores_withdrawal() {
    let mut e = Engine::new();
    freeze_account(&mut e, 1, 100, "10.0");

    let err = e
        .apply(Tx::Withdrawal { client: 1, tx: 201, amount: dec("1.0") })
        .unwrap_err();
    assert_eq!(err, IgnoreReason::AccountFrozen);
}

#[test]
fn frozen_ignores_dispute() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Deposit { client: 1, tx: 101, amount: dec("5.0") }).unwrap();
    freeze_account(&mut e, 1, 102, "1.0");

    let err = e.apply(Tx::Dispute { client: 1, tx: 101 }).unwrap_err();
    assert_eq!(err, IgnoreReason::AccountFrozen);
}

#[test]
fn frozen_ignores_resolve() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();
    freeze_account(&mut e, 1, 102, "1.0");

    let err = e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::AccountFrozen);
}

#[test]
fn frozen_ignores_chargeback() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();
    freeze_account(&mut e, 1, 102, "1.0");

    let err = e.apply(Tx::Chargeback { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::AccountFrozen);
}

// ─── Ignored input: InsufficientFunds ─────────────────────────────────────────

#[test]
fn withdrawal_insufficient_funds() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("1.0") }).unwrap();
    let err = e
        .apply(Tx::Withdrawal { client: 1, tx: 101, amount: dec("2.0") })
        .unwrap_err();
    assert_eq!(err, IgnoreReason::InsufficientFunds);

    let a = account_of(&e, 1);
    assert_eq!(a.available, dec("1.0"));
}

// ─── Ignored input: UnknownTx (no such tx) ────────────────────────────────────

#[test]
fn dispute_unknown_tx() {
    let mut e = Engine::new();
    let err = e.apply(Tx::Dispute { client: 1, tx: 42 }).unwrap_err();
    assert_eq!(err, IgnoreReason::UnknownTx);
}

#[test]
fn resolve_unknown_tx() {
    let mut e = Engine::new();
    let err = e.apply(Tx::Resolve { client: 1, tx: 42 }).unwrap_err();
    assert_eq!(err, IgnoreReason::UnknownTx);
}

#[test]
fn chargeback_unknown_tx() {
    let mut e = Engine::new();
    let err = e.apply(Tx::Chargeback { client: 1, tx: 42 }).unwrap_err();
    assert_eq!(err, IgnoreReason::UnknownTx);
}

// ─── Ignored input: dispute family on a withdrawal tx ─────────────────────────
// Withdrawals are not stored, so a dispute-family row referencing a withdrawal
// tx is structurally indistinguishable from one referencing a non-existent tx.
// Both surface as UnknownTx.

#[test]
fn dispute_on_withdrawal_tx_is_unknown() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Withdrawal { client: 1, tx: 200, amount: dec("3.0") }).unwrap();

    let err = e.apply(Tx::Dispute { client: 1, tx: 200 }).unwrap_err();
    assert_eq!(err, IgnoreReason::UnknownTx);
}

#[test]
fn resolve_on_withdrawal_tx_is_unknown() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Withdrawal { client: 1, tx: 200, amount: dec("3.0") }).unwrap();

    let err = e.apply(Tx::Resolve { client: 1, tx: 200 }).unwrap_err();
    assert_eq!(err, IgnoreReason::UnknownTx);
}

#[test]
fn chargeback_on_withdrawal_tx_is_unknown() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Withdrawal { client: 1, tx: 200, amount: dec("3.0") }).unwrap();

    let err = e.apply(Tx::Chargeback { client: 1, tx: 200 }).unwrap_err();
    assert_eq!(err, IgnoreReason::UnknownTx);
}

// ─── Ignored input: dispute lifecycle violations ──────────────────────────────

#[test]
fn dispute_on_already_disputed() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();

    let err = e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::AlreadyDisputed);
}

#[test]
fn dispute_on_resolved_is_wrong_state() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();
    e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap();

    let err = e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::WrongTxState);
}

// Note: dispute against a ChargedBack deposit is masked by AccountFrozen,
// because chargeback locks the account. This is correct per the plan.
#[test]
fn dispute_on_chargedback_is_masked_by_frozen() {
    let mut e = Engine::new();
    freeze_account(&mut e, 1, 100, "10.0");

    let err = e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::AccountFrozen);
}

#[test]
fn resolve_on_posted_is_wrong_state() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();

    let err = e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::WrongTxState);
}

#[test]
fn resolve_on_resolved_is_wrong_state() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();
    e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap();

    let err = e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::WrongTxState);
}

#[test]
fn resolve_on_chargedback_is_masked_by_frozen() {
    let mut e = Engine::new();
    freeze_account(&mut e, 1, 100, "10.0");

    let err = e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::AccountFrozen);
}

#[test]
fn chargeback_on_posted_is_wrong_state() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();

    let err = e.apply(Tx::Chargeback { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::WrongTxState);
}

#[test]
fn chargeback_on_resolved_is_wrong_state() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();
    e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap();

    let err = e.apply(Tx::Chargeback { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::WrongTxState);
}

#[test]
fn chargeback_on_chargedback_is_masked_by_frozen() {
    let mut e = Engine::new();
    freeze_account(&mut e, 1, 100, "10.0");

    let err = e.apply(Tx::Chargeback { client: 1, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::AccountFrozen);
}

// ─── Ignored input: ClientMismatch ────────────────────────────────────────────

#[test]
fn dispute_client_mismatch() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();

    let err = e.apply(Tx::Dispute { client: 2, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::ClientMismatch);

    // Client 1 untouched; client 2 created with zeros (well-formed row creates account).
    assert_eq!(account_of(&e, 1).available, dec("10.0"));
    let b = account_of(&e, 2);
    assert_eq!(b.available, Decimal::ZERO);
    assert_eq!(b.held, Decimal::ZERO);
    assert!(!b.locked);
}

#[test]
fn resolve_client_mismatch() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();

    let err = e.apply(Tx::Resolve { client: 2, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::ClientMismatch);
}

#[test]
fn chargeback_client_mismatch() {
    let mut e = Engine::new();
    e.apply(Tx::Deposit { client: 1, tx: 100, amount: dec("10.0") }).unwrap();
    e.apply(Tx::Dispute { client: 1, tx: 100 }).unwrap();

    let err = e.apply(Tx::Chargeback { client: 2, tx: 100 }).unwrap_err();
    assert_eq!(err, IgnoreReason::ClientMismatch);

    // Client 1's dispute state untouched: a follow-up valid resolve still works.
    e.apply(Tx::Resolve { client: 1, tx: 100 }).unwrap();
    assert_eq!(account_of(&e, 1).available, dec("10.0"));
}
