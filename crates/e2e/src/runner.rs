//! Main test runner that orchestrates server, Playwright, and visual regression

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};

use crate::error::{E2eError, E2eResult};
use crate::playwright::{PlaywrightConfig, PlaywrightHandle, StepResult};
use crate::server::{ServerConfig, ServerHandle};
use crate::spec::TestSpec;
use crate::visual::{VisualConfig, VisualDiff, VisualTester};

/// Result of running a single test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub success: bool,
    pub duration_ms: u64,
    pub steps: Vec<StepResult>,
    pub visual_diffs: Vec<VisualDiffResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualDiffResult {
    pub name: String,
    pub matches: bool,
    pub diff_percent: f64,
    pub diff_image_path: Option<String>,
}

/// Result of running all tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuiteResult {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration_ms: u64,
    pub results: Vec<TestResult>,
}

/// Main E2E test runner
pub struct TestRunner {
    /// Server configuration
    server_config: ServerConfig,
    
    /// Playwright configuration
    playwright_config: PlaywrightConfig,
    
    /// Visual testing configuration
    visual_config: VisualConfig,
    
    /// Running server handle (if any)
    server: Option<ServerHandle>,
    
    /// Test specs directory
    specs_dir: PathBuf,
    
    /// Output directory for results
    output_dir: PathBuf,
}

impl TestRunner {
    /// Create a new test runner with default configuration
    pub fn new() -> Self {
        Self::with_config(RunnerConfig::default())
    }

    /// Create a test runner with custom configuration
    pub fn with_config(config: RunnerConfig) -> Self {
        Self {
            server_config: config.server,
            playwright_config: config.playwright,
            visual_config: config.visual,
            server: None,
            specs_dir: config.specs_dir,
            output_dir: config.output_dir,
        }
    }

    /// Start the server
    pub async fn start_server(&mut self) -> E2eResult<()> {
        if self.server.is_some() {
            return Ok(()); // Already running
        }

        let server = ServerHandle::spawn(self.server_config.clone()).await?;
        
        // Update playwright config with actual server URL
        self.playwright_config.base_url = server.base_url().to_string();
        
        self.server = Some(server);
        Ok(())
    }

    /// Stop the server
    pub fn stop_server(&mut self) -> E2eResult<()> {
        if let Some(mut server) = self.server.take() {
            server.stop()?;
        }
        Ok(())
    }

    /// Run all tests in the specs directory
    pub async fn run_all(&mut self) -> E2eResult<TestSuiteResult> {
        let specs = TestSpec::load_all(&self.specs_dir)?;
        self.run_specs(&specs).await
    }

    /// Run tests matching a tag
    pub async fn run_tagged(&mut self, tag: &str) -> E2eResult<TestSuiteResult> {
        let specs = TestSpec::load_all(&self.specs_dir)?;
        let filtered: Vec<TestSpec> = specs
            .into_iter()
            .filter(|s| s.tags.contains(&tag.to_string()))
            .collect();
        self.run_specs(&filtered).await
    }

    /// Run a specific test by name
    pub async fn run_test(&mut self, name: &str) -> E2eResult<TestResult> {
        let specs = TestSpec::load_all(&self.specs_dir)?;
        let spec = specs
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| E2eError::SpecParse(format!("Test not found: {}", name)))?;
        
        self.run_spec(&spec).await
    }

    /// Run a list of test specs
    pub async fn run_specs(&mut self, specs: &[TestSpec]) -> E2eResult<TestSuiteResult> {
        let start = Instant::now();
        let mut results = Vec::new();
        let mut passed = 0;
        let mut failed = 0;
        let skipped = 0;

        // Ensure server is running
        self.start_server().await?;

        info!("Running {} test(s)...", specs.len());

        for spec in specs {
            match self.run_spec(spec).await {
                Ok(result) => {
                    if result.success {
                        passed += 1;
                        info!("✓ {} ({} ms)", result.name, result.duration_ms);
                    } else {
                        failed += 1;
                        error!("✗ {} - {}", result.name, result.error.as_deref().unwrap_or("unknown error"));
                    }
                    results.push(result);
                }
                Err(e) => {
                    failed += 1;
                    error!("✗ {} - {}", spec.name, e);
                    results.push(TestResult {
                        name: spec.name.clone(),
                        success: false,
                        duration_ms: 0,
                        steps: vec![],
                        visual_diffs: vec![],
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        info!("");
        info!("Test Results: {} passed, {} failed, {} skipped ({} ms)",
            passed, failed, skipped, duration_ms);

        Ok(TestSuiteResult {
            total: specs.len(),
            passed,
            failed,
            skipped,
            duration_ms,
            results,
        })
    }

    /// Run a single test spec
    pub async fn run_spec(&mut self, spec: &TestSpec) -> E2eResult<TestResult> {
        let start = Instant::now();
        debug!("Running test: {}", spec.name);

        // Update viewport from spec
        let mut pw_config = self.playwright_config.clone();
        pw_config.viewport_width = spec.viewport.width;
        pw_config.viewport_height = spec.viewport.height;

        let playwright = PlaywrightHandle::new(pw_config)?;
        
        let mut step_results = Vec::new();
        let mut test_error: Option<String> = None;
        let mut screenshots: Vec<String> = Vec::new();

        // Execute each step
        for step in &spec.steps {
            let result = playwright.execute_step(step).await?;
            
            if !result.success {
                test_error = result.error.clone();
                step_results.push(result);
                break; // Stop on first failure
            }
            
            // Track screenshots for visual regression
            if let Some(path) = &result.screenshot_path {
                if let Some(name) = path.file_stem() {
                    screenshots.push(name.to_string_lossy().to_string());
                }
            }
            
            step_results.push(result);
        }

        // Visual regression testing
        let mut visual_diffs = Vec::new();
        if spec.visual_regression && test_error.is_none() {
            let visual_tester = VisualTester::new(self.visual_config.clone())?;
            
            for screenshot_name in &screenshots {
                match visual_tester.compare(screenshot_name, Some(spec.visual_threshold)) {
                    Ok(diff) => {
                        if !diff.matches {
                            test_error = Some(format!(
                                "Visual regression in '{}': {:.2}% pixels differ",
                                screenshot_name, diff.diff_percent
                            ));
                        }
                        visual_diffs.push(VisualDiffResult {
                            name: screenshot_name.clone(),
                            matches: diff.matches,
                            diff_percent: diff.diff_percent,
                            diff_image_path: diff.diff_image_path.map(|p| p.to_string_lossy().to_string()),
                        });
                    }
                    Err(E2eError::BaselineNotFound(_)) => {
                        // First run - no baseline yet
                        info!("No baseline for '{}' - will be created on next run with --update-baselines", screenshot_name);
                    }
                    Err(e) => {
                        test_error = Some(format!("Visual comparison error: {}", e));
                    }
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let success = test_error.is_none();

        Ok(TestResult {
            name: spec.name.clone(),
            success,
            duration_ms,
            steps: step_results,
            visual_diffs,
            error: test_error,
        })
    }

    /// Update all visual baselines from current screenshots
    pub fn update_baselines(&self) -> E2eResult<()> {
        let visual_tester = VisualTester::new(VisualConfig {
            auto_update: true,
            ..self.visual_config.clone()
        })?;

        // For each screenshot in actual dir, copy to baseline
        let actual_dir = &self.visual_config.actual_dir;
        
        for entry in std::fs::read_dir(actual_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().map(|e| e == "png").unwrap_or(false) {
                if let Some(name) = path.file_stem() {
                    visual_tester.update_baseline(&name.to_string_lossy())?;
                }
            }
        }
        
        Ok(())
    }

    /// Write test results to JSON file
    pub fn write_results(&self, results: &TestSuiteResult) -> E2eResult<PathBuf> {
        std::fs::create_dir_all(&self.output_dir)?;
        
        let path = self.output_dir.join("test-results.json");
        let json = serde_json::to_string_pretty(results)?;
        std::fs::write(&path, json)?;
        
        info!("Results written to: {}", path.display());
        Ok(path)
    }
}

impl Drop for TestRunner {
    fn drop(&mut self) {
        let _ = self.stop_server();
    }
}

/// Configuration for the test runner
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub server: ServerConfig,
    pub playwright: PlaywrightConfig,
    pub visual: VisualConfig,
    pub specs_dir: PathBuf,
    pub output_dir: PathBuf,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            playwright: PlaywrightConfig::default(),
            visual: VisualConfig::default(),
            specs_dir: PathBuf::from("tests/e2e/specs"),
            output_dir: PathBuf::from("test-results"),
        }
    }
}
