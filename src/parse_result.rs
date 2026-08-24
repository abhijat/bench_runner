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

fn make_table(r: Vec<RunSummary>) -> Table {
    let mut t = Table::new(r);
    t.with(Style::modern_rounded()).clone()
}

pub(crate) fn report_results(run_result: RunResult) -> Result<()> {
    for (bench, results) in run_result {
        println!("{}", bench);
        let results = results
            .into_iter()
            .filter_map(|r| make_summary(r.run_id, &r.dragonfly_id, &r.benchmark_output_file).ok())
            .collect();
        let t = make_table(results);
        println!("{}", t);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_result() {
        let results: Vec<_> = (1..4)
            .map(|n| (n, format!("test_data/mt-{n}.json")))
            .filter_map(|(n, s)| make_summary(n, "D", s.as_ref()).ok())
            .collect();
        let t = make_table(results);
        println!("{t}");
    }
}
