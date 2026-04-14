use std::path::PathBuf;

use anyhow::Context;
use talkiwi_eval::intent_eval;
use talkiwi_eval::reporter::{render_intent_report, render_trace_report};
use talkiwi_eval::trace_eval;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut suite = "intent".to_string();
    let mut fixtures: Option<PathBuf> = None;
    let mut golden: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--suite" => {
                suite = args
                    .next()
                    .context("missing value for --suite, expected intent or trace")?;
            }
            "--fixtures" => {
                fixtures = Some(PathBuf::from(
                    args.next().context("missing value for --fixtures")?,
                ));
            }
            "--golden" => {
                golden = Some(PathBuf::from(
                    args.next().context("missing value for --golden")?,
                ));
            }
            other => anyhow::bail!("unknown arg: {other}"),
        }
    }

    match suite.as_str() {
        "intent" => {
            let result = intent_eval::run_suite(
                fixtures.unwrap_or_else(intent_eval::default_fixture_dir),
                golden.unwrap_or_else(intent_eval::default_golden_dir),
            )
            .await?;
            println!("{}", render_intent_report(&result.metrics));
        }
        "trace" => {
            let result = trace_eval::run_suite(
                fixtures.unwrap_or_else(trace_eval::default_fixture_dir),
                golden.unwrap_or_else(trace_eval::default_golden_dir),
            )?;
            println!("{}", render_trace_report(&result.metrics));
        }
        other => anyhow::bail!("unsupported suite: {other}"),
    }

    Ok(())
}
