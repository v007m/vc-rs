//! `vibranium [base_url] [db_path] [shared_setup_tests]`
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

fn config_from_args() -> cli::Config {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize, default: &str| args.get(i).cloned().unwrap_or_else(|| default.to_owned());
    let upload_key = std::env::var("VIBRANIUM_SECRET_UPLOAD_KEY").unwrap_or_else(|_| "demo-key".to_owned());
    cli::Config {
        base_url: arg(0, "http://127.0.0.1:18080"),
        db_path: PathBuf::from(arg(1, "vibranium-history.db")),
        hooks_root: PathBuf::from(arg(3, "hooks")),
        shared_setup_tests: arg(2, "50").parse().unwrap_or(50),
        concurrency: 32,
        secrets: HashMap::from([("UPLOAD_KEY".to_owned(), upload_key)]),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match cli::run(config_from_args()).await {
        Ok(report) => {
            println!("{report:?}");
            if report.summary.failed + report.summary.errored == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
