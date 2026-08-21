use anyhow::Result;

mod config;
mod process;

fn main() -> Result<()> {
    let mut config = config::load_config()?;
    config.run()?;
    Ok(())
}
