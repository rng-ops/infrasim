//! InfraSim E2E Test Framework
//!
//! This crate provides a Rust-controlled E2E testing framework that:
//! - Spawns the web server as a subprocess
//! - Controls Playwright via its CLI/JSON protocol
//! - Parses declarative YAML test specs
//! - Performs visual regression testing with baseline screenshots
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    E2E Test Runner (Rust)                   │
//! ├─────────────────────────────────────────────────────────────┤
//! │  TestRunner                                                  │
//! │    ├── spawn_server() -> ServerHandle                       │
//! │    ├── spawn_playwright() -> PlaywrightHandle               │
//! │    ├── execute_spec(spec: TestSpec) -> TestResult           │
//! │    └── compare_screenshot(actual, baseline) -> Diff         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  TestSpec (YAML)                                            │
//! │    ├── name, description                                    │
//! │    ├── steps: [Step]                                        │
//! │    │     ├── navigate { url }                               │
//! │    │     ├── click { selector }                             │
//! │    │     ├── fill { selector, value }                       │
//! │    │     ├── wait { selector | timeout_ms }                 │
//! │    │     ├── assert { selector, visible?, text?, attr? }    │
//! │    │     └── screenshot { name, selector? }                 │
//! │    └── visual_baseline: Option<String>                      │
//! └─────────────────────────────────────────────────────────────┘
//! ```

pub mod runner;
pub mod spec;
pub mod visual;
pub mod playwright;
pub mod server;
pub mod error;

pub use runner::TestRunner;
pub use spec::{TestSpec, TestStep};
pub use error::{E2eError, E2eResult};
