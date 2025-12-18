//! Benchmark Commands

use clap::{Args, Subcommand};
use anyhow::Result;
use serde::Serialize;

use crate::client::DaemonClient;
use crate::output::{OutputFormat, TableDisplay, print_list, print_success};
use crate::generated::{BenchmarkRun, BenchmarkResult};

/// Benchmark arguments wrapper
#[derive(Args)]
pub struct BenchmarkArgs {
    #[command(subcommand)]
    pub command: BenchmarkCommands,
}

#[derive(Subcommand)]
pub enum BenchmarkCommands {
    /// Run benchmarks on a VM
    Run {
        /// VM ID to benchmark
        #[arg(short, long)]
        vm_id: String,

        /// Benchmark tests to run (cpu, memory, disk, network, all)
        #[arg(short, long, default_value = "all")]
        tests: String,
    },

    /// List benchmark runs
    List {
        /// Filter by VM ID
        #[arg(long)]
        vm_id: Option<String>,
    },

    /// Get benchmark run details
    Get {
        /// Benchmark run ID
        id: String,
    },
}

/// Benchmark result display wrapper for serialization
#[derive(Serialize)]
pub struct BenchmarkDisplay {
    pub test_name: String,
    pub passed: bool,
    pub score: f64,
    pub unit: String,
    pub duration_ms: i64,
}

impl From<BenchmarkResult> for BenchmarkDisplay {
    fn from(result: BenchmarkResult) -> Self {
        Self {
            test_name: result.test_name,
            passed: result.passed,
            score: result.score,
            unit: result.unit,
            duration_ms: result.duration_ms,
        }
    }
}

impl TableDisplay for BenchmarkDisplay {
    fn headers() -> Vec<&'static str> {
        vec!["Test", "Passed", "Score", "Unit", "Duration"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.test_name.clone(),
            if self.passed { "✓" } else { "✗" }.to_string(),
            format!("{:.2}", self.score),
            self.unit.clone(),
            format!("{}ms", self.duration_ms),
        ]
    }
}

/// Benchmark run display wrapper
#[derive(Serialize)]
pub struct BenchmarkRunDisplay {
    pub id: String,
    pub vm_id: String,
    pub total_tests: usize,
    pub passed_tests: usize,
}

impl From<BenchmarkRun> for BenchmarkRunDisplay {
    fn from(run: BenchmarkRun) -> Self {
        let meta = run.meta.unwrap_or_default();
        let spec = run.spec.unwrap_or_default();
        
        let passed = run.results.iter().filter(|r| r.passed).count();
        
        Self {
            id: meta.id,
            vm_id: spec.vm_id,
            total_tests: run.results.len(),
            passed_tests: passed,
        }
    }
}

impl TableDisplay for BenchmarkRunDisplay {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "VM ID", "Tests", "Passed"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.vm_id.clone(),
            self.total_tests.to_string(),
            self.passed_tests.to_string(),
        ]
    }
}

pub async fn execute(args: BenchmarkArgs, mut client: DaemonClient, format: OutputFormat) -> Result<()> {
    match args.command {
        BenchmarkCommands::Run { vm_id, tests } => {
            let test_list: Vec<String> = if tests == "all" {
                vec!["cpu", "memory", "disk", "network"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            } else {
                tests.split(',').map(|s| s.trim().to_string()).collect()
            };

            let run = client.run_benchmark(&vm_id, test_list).await?;
            
            print_success(&format!("Benchmark completed for VM '{}'", vm_id));
            
            let displays: Vec<BenchmarkDisplay> = run.results
                .into_iter()
                .map(BenchmarkDisplay::from)
                .collect();
            print_list(&displays, format);
        }

        BenchmarkCommands::List { vm_id } => {
            let runs = client.list_benchmark_runs(vm_id).await?;
            let displays: Vec<BenchmarkRunDisplay> = runs.into_iter().map(BenchmarkRunDisplay::from).collect();
            print_list(&displays, format);
        }

        BenchmarkCommands::Get { id } => {
            let run = client.get_benchmark_run(&id).await?;
            
            let displays: Vec<BenchmarkDisplay> = run.results
                .into_iter()
                .map(BenchmarkDisplay::from)
                .collect();
            print_list(&displays, format);
        }
    }

    Ok(())
}
