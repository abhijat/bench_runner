use crate::process::BenchmarkRunResult;
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tabled::settings::Style;
use tabled::{Table, Tabled};

#[derive(Deserialize, Debug)]
struct PercentileLatencies {
    #[serde(alias = "p50.00")]
    p50_latency_ms: f64,

    #[serde(alias = "p99.00")]
    p99_latency_ms: f64,

    #[serde(alias = "p99.90")]
    p999_latency_ms: f64,
}

#[derive(Deserialize, Debug)]
struct Totals {
    #[serde(alias = "Count")]
    ops_count: u64,

    #[serde(alias = "Ops/sec")]
    ops_per_sec: f64,

    #[serde(alias = "KB/sec")]
    kb_per_sec: f64,

    #[serde(alias = "Percentile Latencies")]
    percentile_latencies: PercentileLatencies,
}

#[derive(Deserialize, Debug)]
struct AllStats {
    #[serde(alias = "Totals")]
    totals: Totals,
}

#[derive(Deserialize, Debug)]
struct MTResult {
    #[serde(alias = "ALL STATS")]
    all_stats: AllStats,
}

#[derive(Debug, Tabled)]
struct RunSummary {
    trial: u64,
    #[tabled(rename = "build")]
    dragonfly_name: String,
    #[tabled(rename = "ops/sec")]
    ops_per_sec: f64,
    #[tabled(rename = "kb/sec")]
    kb_per_sec: f64,
    #[tabled(rename = "p50 (ms)")]
    p50_latency_ms: f64,
    #[tabled(rename = "p99 (ms)")]
    p99_latency_ms: f64,
    #[tabled(rename = "p99.9 (ms)")]
    p999_latency_ms: f64,
}

type RunResult = HashMap<String, Vec<BenchmarkRunResult>>;

fn make_summary(index: u64, dragonfly_name: &str, path: &Path) -> Result<RunSummary> {
    let data = fs::read_to_string(path)?;
    let result: MTResult = serde_json::from_str(&data)?;
    Ok(RunSummary {
        trial: index,
        dragonfly_name: dragonfly_name.to_owned(),
        ops_per_sec: result.all_stats.totals.ops_per_sec,
        kb_per_sec: result.all_stats.totals.kb_per_sec,
        p50_latency_ms: result.all_stats.totals.percentile_latencies.p50_latency_ms,
        p99_latency_ms: result.all_stats.totals.percentile_latencies.p99_latency_ms,
        p999_latency_ms: result.all_stats.totals.percentile_latencies.p999_latency_ms,
    })
}

#[derive(Debug, Default)]
struct Range {
    min: f64,
    max: f64,
}

impl Range {
    fn ingest(&mut self, ops_sec: f64) {
        self.min = match self.min {
            0.0 => ops_sec,
            n if n > ops_sec => ops_sec,
            _ => self.min,
        };

        self.max = match self.max {
            0.0 => ops_sec,
            n if n < ops_sec => ops_sec,
            _ => self.max,
        };
    }
}

#[derive(Debug, Default, Tabled)]
struct BuildSummary {
    build_name: String,
    trials: u64,
    #[tabled(rename = "median ops/sec", format = "{:.2}")]
    median_ops_sec: f64,

    #[tabled(rename = "ops/sec range", display = "show_range")]
    ops_sec_range: Range,

    #[tabled(rename = "median p50 (ms)", format = "{:.3}")]
    median_p50: f64,

    #[tabled(rename = "median p99 (ms)", format = "{:.3}")]
    median_p99: f64,

    #[tabled(rename = "median p99.90 (ms)", format = "{:.3}")]
    median_p999: f64,

    #[tabled(skip)]
    ops_sec: Vec<f64>,

    #[tabled(skip)]
    p50: Vec<f64>,

    #[tabled(skip)]
    p99: Vec<f64>,

    #[tabled(skip)]
    p999: Vec<f64>,
}

impl BuildSummary {
    fn ingest(&mut self, rs: &RunSummary) {
        self.trials += 1;
        self.build_name = rs.dragonfly_name.clone();
        self.ops_sec_range.ingest(rs.ops_per_sec);

        self.ops_sec.push(rs.ops_per_sec);

        self.p50.push(rs.p50_latency_ms);
        self.p99.push(rs.p99_latency_ms);
        self.p999.push(rs.p999_latency_ms);
    }

    fn median(values: &mut [f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        values.sort_unstable_by(f64::total_cmp);
        let mid_point = values.len() / 2;
        if values.len() % 2 == 0 {
            (values[mid_point] + values[mid_point - 1]) / 2.0
        } else {
            values[mid_point]
        }
    }

    fn finalize(&mut self) {
        self.median_ops_sec = Self::median(self.ops_sec.as_mut_slice());
        self.median_p50 = Self::median(self.p50.as_mut_slice());
        self.median_p99 = Self::median(self.p99.as_mut_slice());
        self.median_p999 = Self::median(self.p999.as_mut_slice());
    }
}

fn show_range(r: &Range) -> String {
    format!("{}-{}", r.min, r.max)
}

fn summarize_per_build(r: &Vec<RunSummary>) -> HashMap<String, BuildSummary> {
    let mut per_build = HashMap::new();
    for rs in r {
        let entry: &mut BuildSummary = per_build.entry(rs.dragonfly_name.clone()).or_default();
        entry.ingest(&rs);
    }

    for v in per_build.values_mut() {
        v.finalize();
    }
    per_build
}

pub(crate) fn report_results(run_result: RunResult) -> Result<()> {
    let mut deferred = vec![];
    for (bench, results) in run_result {
        println!("{}", bench);
        let results = results
            .into_iter()
            .filter_map(|r| make_summary(r.run_id, &r.dragonfly_id, &r.benchmark_output_file).ok())
            .collect();
        let summary = summarize_per_build(&results);
        let mut t = Table::new(summary.into_values());
        t.with(Style::modern_rounded());
        println!("{t}");

        let mut t = Table::new(results);
        t.with(Style::modern_rounded());
        deferred.push((bench, t));
    }

    println!("\nDetailed Results...\n");

    for (bench, d) in deferred {
        println!("{bench}");
        println!("{d}");
    }
    Ok(())
}
