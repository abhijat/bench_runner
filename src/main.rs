use anyhow::Result;
use crate::parse_result::report_results;

mod bench_runner;
mod parse_result;
mod process;

fn main() -> Result<()> {
    let results = bench_runner::initialize()?.run()?;
    println!();
    report_results(results)?;
    Ok(())
}
