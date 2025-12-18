//! Console Commands

use clap::Parser;
use anyhow::Result;

use crate::client::DaemonClient;
use crate::output::print_success;

#[derive(Parser)]
pub struct ConsoleArgs {
    /// VM ID
    pub vm_id: String,

    /// Open in browser
    #[arg(short, long)]
    pub open: bool,

    /// Just print the URL
    #[arg(short, long)]
    pub url_only: bool,
}

pub async fn execute(args: ConsoleArgs, mut client: DaemonClient) -> Result<()> {
    let url = client.get_console(&args.vm_id).await?;

    if args.url_only {
        println!("{}", url);
    } else {
        print_success(&format!("Console URL: {}", url));

        if args.open {
            // Open in default browser
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("open")
                    .arg(&url)
                    .spawn()?;
                print_success("Opened console in browser");
            }

            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("xdg-open")
                    .arg(&url)
                    .spawn()?;
                print_success("Opened console in browser");
            }

            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("cmd")
                    .args(["/C", "start", &url])
                    .spawn()?;
                print_success("Opened console in browser");
            }
        } else {
            println!("\nTo open in browser, use: infrasim console {} --open", args.vm_id);
        }
    }

    Ok(())
}
