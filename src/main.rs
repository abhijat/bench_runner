use anyhow::Result;

mod bench_runner;
mod process;

fn main() -> Result<()> {
    bench_runner::initialize()?.run()
}
