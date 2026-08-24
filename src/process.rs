use anyhow::Result;
use duct::unix::HandleExt;
use duct::{Handle, cmd};
use rand::{Rng, thread_rng};
use redis::TypedCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, thread};

pub(crate) struct Process {
    handle: Handle,
}

impl Process {
    pub(crate) fn launch(program: &str, args: &[String]) -> Result<Self> {
        let command = cmd(program, args);
        let handle = command
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .start()?;
        Ok(Self { handle })
    }

    pub(crate) fn kill(&self) -> Result<String> {
        self.handle.send_signal(15)?;
        let output = &self.handle.wait()?.stdout;
        let output = String::from_utf8_lossy(output.as_slice())
            .trim()
            .to_string();
        Ok(output)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        match self.kill() {
            Ok(output) => {
                println!("process stopped\n===================================================");
                println!("{output}\n===================================================");
            }
            Err(err) => println!("{}", err),
        }
    }
}

pub(crate) fn run_process(program: &str, args: &[String]) -> Result<()> {
    cmd(program, args).stderr_to_stdout().run()?;
    Ok(())
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

    pub(crate) fn run(&mut self) -> Result<()> {
        for dragonfly_config in &mut self.dragonfly_configs {
            let root = self
                .root
                .join(&self.benchmark_config.id)
                .join(&dragonfly_config.id);

            println!("creating root: {:?}", root);
            fs::create_dir_all(&root)?;

            dragonfly_config.resolve_paths(&root)?;
            let args = dragonfly_config.build_args();
            let process_path = dragonfly_config.path.to_string_lossy();

            println!("starting {process_path} with args: {:?}", args);
            let process = Process::launch(&process_path, &args)?;

            if !wait_ready()? {
                println!("could not start dragonfly!");
                return Ok(());
            }

            let mut benchmark_config = self.benchmark_config.clone();
            benchmark_config.resolve_paths(&root)?;

            let benchmark_path = benchmark_config.path.to_string_lossy();
            let benchmark_args = benchmark_config.build_args();

            println!("starting {benchmark_path} with args: {:?}", benchmark_args);
            run_process(&benchmark_path, &benchmark_args)?;

            drop(process);
            if let Some(data_dir) = dragonfly_config.get("dir") {
                println!("removing data path: {data_dir}");
                fs::remove_dir_all(data_dir)?;
            }

            let dragonfly_json = serde_json::to_string(dragonfly_config)?;
            fs::write(root.join("dragonfly_config.json"), dragonfly_json)?;

            let benchmark_json = serde_json::to_string(&benchmark_config)?;
            fs::write(root.join("benchmark_config.json"), benchmark_json)?;

            thread::sleep(Duration::from_secs(self.cooldown_sec));
        }

        Ok(())
    }
}

fn format_args((k, v): (&String, &String)) -> String {
    if v.is_empty() {
        format!("--{}", k)
    } else {
        format!("--{}={}", k, v)
    }
}

impl ProcessConfig {
    fn resolve_paths(&mut self, root: &Path) -> Result<()> {
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

    fn build_args(&self) -> Vec<String> {
        self.args.iter().map(format_args).collect()
    }

    fn get(&self, key: &str) -> Option<&String> {
        self.args.get(key)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct ProcessConfig {
    pub(crate) path: PathBuf,
    pub(crate) id: String,
    args: HashMap<String, String>,
}
