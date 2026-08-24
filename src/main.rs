use crate::parse_result::report_results;
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

mod bench_runner;
mod parse_result;
mod process;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    config_file: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let results = bench_runner::initialize(&args.config_file)?.run()?;
    println!();
    report_results(results)?;
    Ok(())
}
