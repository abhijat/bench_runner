use crate::config::ProcessConfig;
use anyhow::Result;
use duct::unix::HandleExt;
use duct::{Handle, cmd};
use rand::{Rng, thread_rng};
use redis::TypedCommands;
use std::path::PathBuf;
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
    benchmark_config: ProcessConfig,
    dragonfly_configs: Vec<ProcessConfig>,
}

impl BenchmarkRun {
    pub(crate) fn new(
        root: PathBuf,
        benchmark_config: ProcessConfig,
        mut dragonfly_configs: Vec<ProcessConfig>,
    ) -> Self {
        thread_rng().shuffle(&mut dragonfly_configs);
        Self {
            root,
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
            let _process = Process::launch(&process_path, &args)?;

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
        }

        Ok(())
    }
}
