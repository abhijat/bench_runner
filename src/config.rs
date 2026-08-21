use crate::process::BenchmarkRun;
use anyhow::Result;
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
    pub(crate) dragonflies: Vec<ProcessConfig>,
    pub(crate) benchmarks: Vec<ProcessConfig>,
}

impl RunConfig {
    fn initialize(&mut self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    pub(crate) fn make_run_root(&self, dragonfly_id: &str, benchmark_id: &str) -> Result<PathBuf> {
        Ok(self.root.join(dragonfly_id).join(benchmark_id))
    }

    pub(crate) fn run(&mut self, count: usize) -> Result<()> {
        self.initialize()?;
        for n in 0..count {
            let root_for_run = self.root.join("runs").join(n.to_string());
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
    let mut parsed: RunConfig = toml::from_str(&data)?;
    parsed.initialize()?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

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
