use anyhow::{Result, anyhow};
use duct::unix::HandleExt;
use duct::{Handle, cmd};
use rand::{Rng, thread_rng};
use redis::TypedCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, thread};

struct Process {
    handle: Handle,
}

impl Process {
    fn launch(program: &str, args: &[String]) -> Result<Self> {
        let command = cmd(program, args);
        let handle = command
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .start()?;
        Ok(Self { handle })
    }

    fn kill(&self) -> Result<String> {
        self.handle.send_signal(15)?;
        let output = &self.handle.wait()?.stdout;
        let output = String::from_utf8_lossy(output.as_slice())
            .trim()
            .to_string();
        Ok(output)
    }

    fn one_shot(program: &str, args: &[String]) -> Result<()> {
        cmd(program, args).stderr_to_stdout().run()?;
        Ok(())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Err(err) = self.kill() {
            println!("{err}");
        }
    }
}

fn wait_ready() -> Result<bool> {
    let mut client = redis::Client::open("redis://127.0.0.1/")?;
    let start = Instant::now();
    let limit = Duration::from_secs(5);
    while Instant::now() - start < limit {
        match client.ping() {
            Ok(resp) if resp == "PONG" => return Ok(true),
            Ok(resp) => println!("bad response {resp}... will retry"),
            Err(err) => println!("{err}... will retry"),
        }
        thread::sleep(Duration::from_millis(200));
    }
    Ok(false)
}

pub(crate) struct BenchmarkRun {
    root: PathBuf,
    cooldown_sec: u64,
    benchmark_config: ProcessConfig,
    dragonfly_configs: Vec<ProcessConfig>,
}

#[derive(Debug)]
pub(crate) struct BenchmarkRunResult {
    pub(crate) run_id: u64,
    pub(crate) dragonfly_id: String,
    pub(crate) benchmark_output_file: PathBuf,
}

impl BenchmarkRun {
    pub(crate) fn new(
        root: PathBuf,
        sleep_sec: u64,
        benchmark_config: ProcessConfig,
        mut dragonfly_configs: Vec<ProcessConfig>,
    ) -> Self {
        thread_rng().shuffle(&mut dragonfly_configs);
        Self {
            root,
            cooldown_sec: sleep_sec,
            benchmark_config,
            dragonfly_configs,
        }
    }

    pub(crate) fn run(&mut self, run_id: u64) -> Result<Vec<BenchmarkRunResult>> {
        let mut results = vec![];
        let len = self.dragonfly_configs.len();
        for (n, dragonfly_config) in self.dragonfly_configs.iter_mut().enumerate() {
            let root = self
                .root
                .join(&self.benchmark_config.id)
                .join(&dragonfly_config.id);

            fs::create_dir_all(&root)?;

            dragonfly_config.resolve_paths(&root)?;
            let args = dragonfly_config.build_process_args();
            let process_path = dragonfly_config.path.to_string_lossy();

            println!("starting {process_path} with args: {:?}", args);
            let process = Process::launch(&process_path, &args)?;

            if !wait_ready()? {
                println!("could not start dragonfly!");
                return Ok(results);
            }

            let mut benchmark_config = self.benchmark_config.clone();
            benchmark_config.resolve_paths(&root)?;

            let benchmark_path = benchmark_config.path.to_string_lossy();

            if benchmark_config.warmup.is_some() {
                let warmup_args = benchmark_config.build_warmup_args();
                println!(
                    "starting warmup {benchmark_path} with args: {:?}",
                    warmup_args
                );
                Process::one_shot(&benchmark_path, &warmup_args)?;
                thread::sleep(Duration::from_secs(2));
            }

            let benchmark_args = benchmark_config.build_process_args();

            println!("starting {benchmark_path} with args: {:?}", benchmark_args);
            Process::one_shot(&benchmark_path, &benchmark_args)?;

            drop(process);
            if let Some(data_dir) = dragonfly_config.get("dir") {
                println!("removing data path: {data_dir}");
                fs::remove_dir_all(data_dir)?;
            }

            let dragonfly_json = serde_json::to_string(dragonfly_config)?;
            let dragonfly_config_json = "dragonfly_config.json";
            fs::write(root.join(dragonfly_config_json), dragonfly_json)?;

            let benchmark_json = serde_json::to_string(&benchmark_config)?;
            let benchmark_config_path = "benchmark_config.json";
            fs::write(root.join(benchmark_config_path), benchmark_json)?;

            if n < len - 1 {
                thread::sleep(Duration::from_secs(self.cooldown_sec));
            }

            results.push(BenchmarkRunResult {
                run_id,
                dragonfly_id: dragonfly_config.id.clone(),
                benchmark_output_file: root.join(benchmark_config.get("json-out-file").unwrap()),
            });
        }

        Ok(results)
    }
}

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
    warmup: Option<HashMap<String, String>>,
}

impl ProcessConfig {
    fn resolve_paths(&mut self, root: &Path) -> Result<()> {
        for v in self.args.values_mut() {
            if let Some(suffix) = v.strip_prefix("ROOT+") {
                *v = root.join(suffix).into_string().unwrap();
            }
        }
        Ok(())
    }

    fn build_args(&self, m: &HashMap<String, String>) -> Vec<String> {
        m.iter().map(format_args).collect()
    }

    fn build_warmup_args(&self) -> Vec<String> {
        self.build_args(self.warmup.as_ref().unwrap())
    }

    fn build_process_args(&self) -> Vec<String> {
        self.build_args(&self.args)
    }

    fn get(&self, key: &str) -> Option<&String> {
        self.args.get(key)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let path = self.path.to_string_lossy();

        let metadata = self.path.metadata()?;
        if !metadata.is_file() {
            return Err(anyhow!("{} is not a file", path));
        }

        if metadata.permissions().mode() & 0o111 == 0 {
            Err(anyhow!("{} is not an executable", path))
        } else {
            Ok(())
        }
    }
}
