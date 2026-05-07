use std::io::{self, BufReader, BufWriter, Read, Write};

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::domain::{Account, Tx};
use crate::engine::Engine;

#[derive(Debug, Deserialize)]
struct InputRow {
    #[serde(rename = "type")]
    kind: String,
    client: u16,
    tx: u32,
    amount: Option<String>,
}

#[derive(Debug, Serialize)]
struct OutputRow {
    client: u16,
    available: String,
    held: String,
    total: String,
    locked: bool,
}

#[derive(Debug)]
enum InputReject {
    UnknownKind,
    AmountMissing,
    AmountUnexpected,
    AmountNegative,
    AmountTooPrecise,
    AmountUnparsable,
}

impl std::fmt::Display for InputReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            InputReject::UnknownKind => "unknown transaction kind",
            InputReject::AmountMissing => "amount missing",
            InputReject::AmountUnexpected => "amount not allowed for this transaction kind",
            InputReject::AmountNegative => "amount is negative",
            InputReject::AmountTooPrecise => "amount has more than four decimal places",
            InputReject::AmountUnparsable => "amount is not a valid decimal",
        };
        f.write_str(s)
    }
}

fn try_into_tx(row: &InputRow) -> Result<Tx, InputReject> {
    let amount_str = row
        .amount
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    match row.kind.as_str() {
        "deposit" => {
            let s = amount_str.ok_or(InputReject::AmountMissing)?;
            let amount = parse_amount(s)?;
            Ok(Tx::Deposit { client: row.client, tx: row.tx, amount })
        }
        "withdrawal" => {
            let s = amount_str.ok_or(InputReject::AmountMissing)?;
            let amount = parse_amount(s)?;
            Ok(Tx::Withdrawal { client: row.client, tx: row.tx, amount })
        }
        "dispute" => {
            reject_if_amount_present(amount_str)?;
            Ok(Tx::Dispute { client: row.client, tx: row.tx })
        }
        "resolve" => {
            reject_if_amount_present(amount_str)?;
            Ok(Tx::Resolve { client: row.client, tx: row.tx })
        }
        "chargeback" => {
            reject_if_amount_present(amount_str)?;
            Ok(Tx::Chargeback { client: row.client, tx: row.tx })
        }
        _ => Err(InputReject::UnknownKind),
    }
}

fn reject_if_amount_present(amount: Option<&str>) -> Result<(), InputReject> {
    if amount.is_some() {
        Err(InputReject::AmountUnexpected)
    } else {
        Ok(())
    }
}

fn parse_amount(s: &str) -> Result<Decimal, InputReject> {
    if let Some(idx) = s.find('.')
        && s[idx + 1..].len() > 4
    {
        return Err(InputReject::AmountTooPrecise);
    }
    let d = Decimal::from_str_exact(s).map_err(|_| InputReject::AmountUnparsable)?;
    if d < Decimal::ZERO {
        return Err(InputReject::AmountNegative);
    }
    Ok(d)
}

pub fn run<R: Read, W: Write, E: Write>(
    input: R,
    output: W,
    mut err: E,
) -> io::Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .flexible(false)
        .from_reader(BufReader::new(input));

    let mut engine = Engine::new();

    for (i, result) in reader.deserialize::<InputRow>().enumerate() {
        let row_number = i + 1;
        match result {
            Err(e) => {
                if matches!(e.kind(), csv::ErrorKind::Io(_)) {
                    return Err(io::Error::other(e));
                }
                writeln!(err, "ignored row {row_number}: parse error ({e})")?;
            }
            Ok(row) => match try_into_tx(&row) {
                Err(reason) => {
                    writeln!(err, "ignored row {row_number}: {reason} [{row:?}]")?;
                }
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

fn write_accounts<'a, W, I>(w: W, accounts: I) -> io::Result<()>
where
    W: Write,
    I: Iterator<Item = &'a Account>,
{
    let mut wtr = csv::Writer::from_writer(w);
    for a in accounts {
        wtr.serialize(OutputRow {
            client: a.client,
            available: format_4dp(a.available),
            held: format_4dp(a.held),
            total: format_4dp(a.total()),
            locked: a.locked,
        })?;
    }
    wtr.flush()
}

fn format_4dp(d: Decimal) -> String {
    // round_dp_with_strategy normalizes the value but may leave scale < 4
    // (e.g. integer or 1dp inputs). rescale pads trailing zeros without
    // changing the numeric value.
    let mut d = d.round_dp_with_strategy(4, RoundingStrategy::MidpointNearestEven);
    d.rescale(4);
    d.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str_exact(s).expect("valid decimal in test")
    }

    #[test]
    fn format_4dp_pads_integer_value() {
        assert_eq!(format_4dp(dec("10")), "10.0000");
    }

    #[test]
    fn format_4dp_pads_one_decimal() {
        assert_eq!(format_4dp(dec("1.5")), "1.5000");
    }

    #[test]
    fn format_4dp_keeps_four_decimals() {
        assert_eq!(format_4dp(dec("1.2345")), "1.2345");
    }

    #[test]
    fn format_4dp_rounds_banker_half_to_even() {
        assert_eq!(format_4dp(dec("1.23455")), "1.2346");
        assert_eq!(format_4dp(dec("1.23445")), "1.2344");
    }

    #[test]
    fn parse_amount_rejects_over_precision() {
        assert!(matches!(parse_amount("1.23456"), Err(InputReject::AmountTooPrecise)));
    }

    #[test]
    fn parse_amount_rejects_negative() {
        assert!(matches!(parse_amount("-1.0"), Err(InputReject::AmountNegative)));
    }

    #[test]
    fn parse_amount_accepts_zero() {
        assert_eq!(parse_amount("0").unwrap(), Decimal::ZERO);
    }

    #[test]
    fn parse_amount_accepts_four_decimal_places() {
        assert_eq!(parse_amount("1.2345").unwrap(), dec("1.2345"));
    }

    #[test]
    fn try_into_tx_deposit_requires_amount() {
        let row = InputRow { kind: "deposit".into(), client: 1, tx: 1, amount: None };
        assert!(matches!(try_into_tx(&row), Err(InputReject::AmountMissing)));
    }

    #[test]
    fn try_into_tx_dispute_rejects_amount() {
        let row = InputRow {
            kind: "dispute".into(),
            client: 1,
            tx: 1,
            amount: Some("1.0".into()),
        };
        assert!(matches!(try_into_tx(&row), Err(InputReject::AmountUnexpected)));
    }

    #[test]
    fn try_into_tx_treats_empty_amount_string_as_absent_for_lifecycle() {
        let row = InputRow { kind: "dispute".into(), client: 1, tx: 1, amount: Some("".into()) };
        assert!(matches!(try_into_tx(&row), Ok(Tx::Dispute { client: 1, tx: 1 })));
    }

    #[test]
    fn try_into_tx_unknown_kind() {
        let row = InputRow { kind: "frobnicate".into(), client: 1, tx: 1, amount: None };
        assert!(matches!(try_into_tx(&row), Err(InputReject::UnknownKind)));
    }
}
