use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use assert_cmd::Command;

#[derive(Debug, serde::Deserialize, PartialEq, Eq, Hash)]
struct OutputRow {
    client: u16,
    available: String,
    held: String,
    total: String,
    locked: bool,
}

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(name);
    p
}

fn read_fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).expect("fixture exists")
}

fn parse_output(bytes: &[u8]) -> HashMap<u16, OutputRow> {
    csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_reader(bytes)
        .deserialize::<OutputRow>()
        .map(|r| r.expect("parses output row"))
        .map(|r| (r.client, r))
        .collect()
}

fn assert_outputs_match(actual: &[u8], expected: &[u8]) {
    let a = parse_output(actual);
    let e = parse_output(expected);
    let a_keys: HashSet<_> = a.keys().copied().collect();
    let e_keys: HashSet<_> = e.keys().copied().collect();
    assert_eq!(a_keys, e_keys, "client set mismatch\nactual={a:?}\nexpected={e:?}");
    for (k, ev) in &e {
        assert_eq!(a.get(k), Some(ev), "row mismatch for client {k}");
    }
}

fn run_fixture(name: &str) -> (Vec<u8>, Vec<u8>) {
    let input = read_fixture(name);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    basic_payment_engine::run(input.as_slice(), &mut stdout, &mut stderr)
        .expect("run succeeds on fixture");
    (stdout, stderr)
}

fn assert_fixture_matches(input_name: &str, expected_name: &str) {
    let (actual, _stderr) = run_fixture(input_name);
    let expected = read_fixture(expected_name);
    assert_outputs_match(&actual, &expected);
}

// ─── Layer A: csv_io::run in-process ──────────────────────────────────────────

#[test]
fn happy_path_matches_expected() {
    assert_fixture_matches("happy_path.csv", "happy_path.expected.csv");
}

#[test]
fn dispute_lifecycle_matches_expected() {
    assert_fixture_matches("dispute_lifecycle.csv", "dispute_lifecycle.expected.csv");
}

#[test]
fn chargeback_freezes_matches_expected() {
    assert_fixture_matches("chargeback_freezes.csv", "chargeback_freezes.expected.csv");
}

#[test]
fn multi_client_matches_expected() {
    assert_fixture_matches("multi_client.csv", "multi_client.expected.csv");
}

#[test]
fn whitespace_and_precision_matches_expected() {
    assert_fixture_matches("whitespace_and_precision.csv", "whitespace_and_precision.expected.csv");
}

#[test]
fn malformed_rows_matches_expected() {
    assert_fixture_matches("malformed_rows.csv", "malformed_rows.expected.csv");
}

// ─── Layer A: stderr surfaces ignored rows ───────────────────────────────────

#[test]
fn malformed_rows_emits_stderr_for_each_rejection() {
    let (_stdout, stderr) = run_fixture("malformed_rows.csv");
    let s = std::str::from_utf8(&stderr).expect("utf8 stderr");

    // One line per ignored row; substring assertions only — exact format is
    // tweakable without test churn.
    assert!(s.contains("unknown transaction kind"), "stderr={s}");
    assert!(s.contains("amount missing"), "stderr={s}");
    assert!(s.contains("more than four decimal places"), "stderr={s}");
    assert!(s.contains("amount is negative"), "stderr={s}");
    assert!(s.contains("amount not allowed"), "stderr={s}");
    assert!(s.contains("parse error"), "stderr={s}");
}

#[test]
fn chargeback_freezes_emits_stderr_for_post_freeze_rows() {
    let (_stdout, stderr) = run_fixture("chargeback_freezes.csv");
    let s = std::str::from_utf8(&stderr).expect("utf8 stderr");
    assert!(s.contains("AccountFrozen"), "stderr={s}");
}

#[test]
fn multi_client_emits_stderr_for_insufficient_funds() {
    let (_stdout, stderr) = run_fixture("multi_client.csv");
    let s = std::str::from_utf8(&stderr).expect("utf8 stderr");
    assert!(s.contains("InsufficientFunds"), "stderr={s}");
}

// ─── Layer B: end-to-end binary smoke + CLI negative tests ───────────────────

#[test]
fn binary_smoke_test_processes_happy_path() {
    let path = fixture("happy_path.csv");
    let output = Command::cargo_bin("basic_payment_engine")
        .unwrap()
        .arg(&path)
        .output()
        .expect("binary runs");
    assert!(output.status.success(), "exit status: {:?}", output.status);
    let expected = read_fixture("happy_path.expected.csv");
    assert_outputs_match(&output.stdout, &expected);
}

#[test]
fn binary_no_args_exits_non_zero_with_usage() {
    let output = Command::cargo_bin("basic_payment_engine")
        .unwrap()
        .output()
        .expect("binary runs");
    assert!(!output.status.success());
    let stderr = std::str::from_utf8(&output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("Usage:"), "stderr={stderr}");
}

#[test]
fn binary_bad_path_exits_non_zero_with_open_error() {
    let output = Command::cargo_bin("basic_payment_engine")
        .unwrap()
        .arg("/nonexistent/path/to/transactions.csv")
        .output()
        .expect("binary runs");
    assert!(!output.status.success());
    let stderr = std::str::from_utf8(&output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("cannot open"), "stderr={stderr}");
}
