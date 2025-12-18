//! Output formatting for CLI

use clap::ValueEnum;
use comfy_table::{Table, ContentArrangement, presets::UTF8_FULL};
use serde::Serialize;

/// Output format
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    /// Human-readable table format
    #[default]
    Table,
    /// JSON format
    Json,
    /// YAML format
    Yaml,
    /// Plain text format
    Plain,
}

/// Trait for items that can be displayed in a table
pub trait TableDisplay {
    fn headers() -> Vec<&'static str>;
    fn row(&self) -> Vec<String>;
}

/// Print a single item
pub fn print_item<T: Serialize + TableDisplay>(item: &T, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            
            table.set_header(T::headers());
            table.add_row(item.row());
            
            println!("{table}");
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(item).unwrap_or_default());
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(item).unwrap_or_default());
        }
        OutputFormat::Plain => {
            let row = item.row();
            for (header, value) in T::headers().iter().zip(row.iter()) {
                println!("{}: {}", header, value);
            }
        }
    }
}

/// Print a list of items
pub fn print_list<T: Serialize + TableDisplay>(items: &[T], format: OutputFormat) {
    if items.is_empty() {
        println!("No items found.");
        return;
    }

    match format {
        OutputFormat::Table => {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            
            table.set_header(T::headers());
            for item in items {
                table.add_row(item.row());
            }
            
            println!("{table}");
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(items).unwrap_or_default());
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(items).unwrap_or_default());
        }
        OutputFormat::Plain => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    println!("---");
                }
                let row = item.row();
                for (header, value) in T::headers().iter().zip(row.iter()) {
                    println!("{}: {}", header, value);
                }
            }
        }
    }
}

/// Print a simple message
pub fn print_message(message: &str, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!(r#"{{"message": "{}"}}"#, message);
        }
        _ => {
            println!("{}", message);
        }
    }
}

/// Print success message
pub fn print_success(message: &str) {
    println!("✅ {}", message);
}

/// Print error message
pub fn print_error(message: &str) {
    eprintln!("❌ {}", message);
}

/// Print warning message
pub fn print_warning(message: &str) {
    println!("⚠️  {}", message);
}

/// Print info message
pub fn print_info(message: &str) {
    println!("ℹ️  {}", message);
}

// Add serde_yaml dependency for YAML output
mod serde_yaml {
    pub fn to_string<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, ()> {
        // Simple YAML-like output for now
        // In production, use the actual serde_yaml crate
        serde_json::to_string_pretty(value).map_err(|_| ())
    }
}
