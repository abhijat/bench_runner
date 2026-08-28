use crate::process::{BenchmarkRun, BenchmarkRunResult, ProcessConfig};
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct BenchRunner {
    root: PathBuf,
    pub(crate) cooldown_sec: u64,
    n_runs: u64,

    pub(crate) dragonflies: Vec<ProcessConfig>,
    pub(crate) benchmarks: Vec<ProcessConfig>,
}

impl BenchRunner {
    pub(crate) fn run(&mut self) -> Result<HashMap<String, Vec<BenchmarkRunResult>>> {
        let now = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let mut results: HashMap<String, Vec<BenchmarkRunResult>> = HashMap::new();
        for n in 0..self.n_runs {
            let root_for_run = self.root.join("runs").join(&now).join(n.to_string());
            for benchmark_config in &self.benchmarks {
                let mut benchmark_run = BenchmarkRun::new(
                    root_for_run.clone(),
                    self.cooldown_sec,
                    benchmark_config.clone(),
                    self.dragonflies.clone(),
                );
                let r = benchmark_run.run(n)?;
                results
                    .entry(benchmark_config.id.clone())
                    .or_default()
                    .extend(r.into_iter());
            }
        }
        Ok(results)
    }

    fn validate(&self) -> Result<()> {
        self.dragonflies
            .iter()
            .chain(self.benchmarks.iter())
            .try_for_each(ProcessConfig::validate)
    }
}

pub(crate) fn initialize(p: &Path) -> Result<BenchRunner> {
    let data = fs::read_to_string(p)?;
    let parsed: BenchRunner = toml::from_str(&data)?;
    parsed.validate()?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_from_toml() {
        let s = fs::read_to_string("config/runner_config.toml").unwrap();
        let config: BenchRunner = toml::from_str(&s).unwrap();
        assert_eq!(config.dragonflies.len(), 2);
        assert_eq!(config.dragonflies[0].id, "normal-build");
        assert_eq!(config.benchmarks.len(), 2);
        assert_eq!(config.benchmarks[0].id, "write-only-256-1");
        assert_eq!(config.benchmarks[1].id, "write-only-256-32");
    }
}
