use anyhow::{Result, anyhow};
use duct::unix::HandleExt;
use duct::{Handle, cmd};
use redis::TypedCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env::home_dir;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::{fs, thread};

const MT: &str = "memtier_benchmark";

fn resolve_path(path: &str) -> Result<String> {
    if path.starts_with("H+") {
        let home = home_dir().ok_or(anyhow!("no home dir for user"))?;
        let rest = path.strip_prefix("H+").unwrap();
        return Ok(home.join(rest).into_string().unwrap());
    }
    Ok(path.to_owned())
}

#[derive(Serialize, Deserialize, Debug)]
struct RunConfig {
    root: PathBuf,
    dragonflies: Vec<HashMap<String, String>>,
    benchmarks: Vec<HashMap<String, String>>,
}

impl RunConfig {
    fn initialize(&mut self) -> Result<()> {
        let root = resolve_path(self.root.to_str().unwrap())?;
        self.root = PathBuf::from_str(&root)?;
        if !self.root.exists() {
            fs::create_dir_all(&self.root)?;
        }
        Ok(())
    }

    fn determine_root(&self, dragonfly_id: usize, benchmark_id: usize) -> Result<String> {
        Ok(self
            .root
            .join("dragonfly")
            .join(dragonfly_id.to_string())
            .join(benchmark_id.to_string())
            .into_string()
            .unwrap())
    }
}

fn build_args(map: &HashMap<String, String>) -> Vec<String> {
    let mut args = Vec::new();
    for (k, v) in map {
        if k == "PATH" {
            continue;
        }
        if v.is_empty() {
            args.push(format!("--{k}"));
        } else {
            args.push(format!("--{k}={v}"))
        }
    }
    args
}

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

    fn run(args: &[String]) -> Result<()> {
        cmd(MT, args).stderr_to_stdout().run()?;
        Ok(())
    }

    fn kill(&self) -> Result<String> {
        self.handle.send_signal(15)?;
        let output = &self.handle.wait()?.stdout;
        let output = String::from_utf8_lossy(output.as_slice())
            .trim()
            .to_string();
        Ok(output)
    }
}

fn load_config() -> Result<RunConfig> {
    let data = fs::read_to_string("config/config.toml")?;
    let mut parsed: RunConfig = toml::from_str(&data)?;
    parsed.initialize()?;
    Ok(parsed)
}

fn wait_ready() -> Result<bool> {
    let mut client = redis::Client::open("redis://127.0.0.1/")?;
    let start = Instant::now();
    let limit = Duration::from_secs(5);
    while Instant::now() - start < limit {
        match client.ping() {
            Ok(resp) => {
                if resp == "PONG" {
                    return Ok(true);
                } else {
                    println!("bad response {resp}");
                }
            }
            Err(err) => {
                println!("{err}... will retry");
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Ok(false)
}

fn run_cycle(config: RunConfig) -> Result<()> {
    for (dragonfly_id, dragonfly) in config.dragonflies.iter().enumerate() {
        let mut dragonfly_args = build_args(dragonfly);
        let path = resolve_path(&dragonfly["PATH"])?;

        for (benchmark_id, benchmark) in config.benchmarks.iter().enumerate() {
            let root = config.determine_root(dragonfly_id, benchmark_id)?;
            dragonfly_args.push(format!("--dir={root}"));

            println!("starting dragonfly with args: {dragonfly_args:?}");

            let dragonfly = Process::launch(&path, dragonfly_args.as_slice())?;
            if !wait_ready()? {
                println!("failed to start up dragonfly. Exiting");
                break;
            }

            println!("process is ready, starting benchmark");

            let benchmark_args = build_args(benchmark);
            Process::run(benchmark_args.as_slice())?;

            let dragonfly_out = dragonfly.kill()?;
            println!("process stopped: {}", dragonfly_out);
            dragonfly_args.pop();
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let config = load_config()?;
    run_cycle(config)?;
    Ok(())
}
