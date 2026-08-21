use crate::process::BenchmarkRun;
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn format_args((k, v): (&String, &String)) -> String {
    if v.is_empty() {
        format!("--{}", k)
    } else {
        format!("--{}={}", k, v)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct ProcessConfig {
    pub(crate) path: PathBuf,
    pub(crate) id: String,
    args: HashMap<String, String>,
}

impl ProcessConfig {
    pub(crate) fn resolve_paths(&mut self, root: &Path) -> Result<()> {
        for v in self.args.values_mut() {
            if v.starts_with("ROOT+") {
                *v = root
                    .join(v.strip_prefix("ROOT+").unwrap())
                    .into_string()
                    .unwrap();
            }
        }
        Ok(())
    }

    pub(crate) fn build_args(&self) -> Vec<String> {
        self.args.iter().map(format_args).collect()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RunConfig {
    root: PathBuf,
    pub(crate) cooldown_sec: u64,
    n_runs: u64,

    pub(crate) dragonflies: Vec<ProcessConfig>,
    pub(crate) benchmarks: Vec<ProcessConfig>,
}

impl RunConfig {
    pub(crate) fn run(&mut self) -> Result<()> {
        for n in 0..self.n_runs {
            let now = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
            let root_for_run = self.root.join("runs").join(now).join(n.to_string());

            println!("starting run: {n} in {root_for_run:?}");

            for benchmark_config in &self.benchmarks {
                let mut benchmark_run = BenchmarkRun::new(
                    root_for_run.clone(),
                    benchmark_config.clone(),
                    self.dragonflies.clone(),
                );
                benchmark_run.run()?;
            }
        }
        Ok(())
    }
}

pub(crate) fn load_config() -> Result<RunConfig> {
    let data = fs::read_to_string("config/config.toml")?;
    let parsed = toml::from_str(&data)?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn convert_from_toml() {
        let s = fs::read_to_string("config/config.toml").unwrap();
        let config: RunConfig = toml::from_str(&s).unwrap();
        assert_eq!(config.dragonflies.len(), 1);
        assert_eq!(config.dragonflies[0].id, "normal-build");
        assert_eq!(config.benchmarks.len(), 2);
        assert_eq!(config.benchmarks[0].id, "write-only-256-1");
        assert_eq!(config.benchmarks[1].id, "write-only-256-32");
    }
}
