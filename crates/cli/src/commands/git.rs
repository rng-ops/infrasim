//! Git Commands
//!
//! Developer utilities for working with git branches.

use clap::{Args, Subcommand};
use anyhow::{Result, Context};
use serde::Serialize;
use std::process::Command;

use crate::output::{OutputFormat, TableDisplay, print_list};

#[derive(Subcommand)]
pub enum GitCommands {
    /// List branches sorted by last update time
    Branches(BranchesArgs),
}

#[derive(Args)]
pub struct BranchesArgs {
    /// Include remote branches
    #[arg(short, long)]
    pub remotes: bool,

    /// Show only the N most recently updated branches
    #[arg(short, long)]
    pub limit: Option<usize>,

    /// Show all branches (local and remote)
    #[arg(short, long)]
    pub all: bool,
}

/// Branch information for display
#[derive(Serialize, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub last_commit: String,
    pub last_commit_date: String,
    pub author: String,
}

impl TableDisplay for BranchInfo {
    fn headers() -> Vec<&'static str> {
        vec!["Branch", "Last Commit", "Date", "Author"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            if self.last_commit.len() > 10 {
                self.last_commit[..10].to_string()
            } else {
                self.last_commit.clone()
            },
            self.last_commit_date.clone(),
            self.author.clone(),
        ]
    }
}

pub async fn execute(cmd: GitCommands, format: OutputFormat) -> Result<()> {
    match cmd {
        GitCommands::Branches(args) => execute_branches(args, format).await,
    }
}

async fn execute_branches(args: BranchesArgs, format: OutputFormat) -> Result<()> {
    let mut branches = get_branches(args.remotes || args.all)?;
    
    // Sort by commit date (most recent first)
    branches.sort_by(|a, b| b.last_commit_date.cmp(&a.last_commit_date));
    
    // Apply limit if specified
    if let Some(limit) = args.limit {
        branches.truncate(limit);
    }
    
    print_list(&branches, format);
    
    Ok(())
}

fn get_branches(include_remotes: bool) -> Result<Vec<BranchInfo>> {
    let branch_args = if include_remotes {
        vec!["branch", "-a", "--format=%(refname:short)"]
    } else {
        vec!["branch", "--format=%(refname:short)"]
    };

    let output = Command::new("git")
        .args(&branch_args)
        .output()
        .context("Failed to execute git branch command")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git branch failed: {}", error);
    }

    let branch_names = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in git branch output")?;

    let mut branches = Vec::new();

    for branch_name in branch_names.lines() {
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            continue;
        }

        // Get last commit info for this branch
        let log_output = Command::new("git")
            .args([
                "log",
                "-1",
                "--format=%H%n%ci%n%an",
                branch_name,
            ])
            .output()
            .context(format!("Failed to get log for branch {}", branch_name))?;

        if !log_output.status.success() {
            continue; // Skip branches we can't get info for
        }

        let log_info = String::from_utf8_lossy(&log_output.stdout);
        let lines: Vec<&str> = log_info.lines().collect();

        if lines.len() >= 3 {
            let commit_hash = lines[0].to_string();
            let commit_date = lines[1].to_string();
            let author = lines[2].to_string();

            branches.push(BranchInfo {
                name: branch_name.to_string(),
                last_commit: commit_hash,
                last_commit_date: commit_date,
                author,
            });
        }
    }

    Ok(branches)
}
