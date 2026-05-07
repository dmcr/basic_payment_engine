use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let path = match args.as_slice() {
        [_, p] => p,
        _ => {
            let prog = args.first().map(String::as_str).unwrap_or("payment-engine");
            eprintln!("Usage: {prog} <transactions.csv>");
            return ExitCode::from(1);
        }
    };

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: cannot open {path}: {e}");
            return ExitCode::from(1);
        }
    };

    if let Err(e) = basic_payment_engine::run(file, std::io::stdout(), std::io::stderr()) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
