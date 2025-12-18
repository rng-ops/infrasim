//! Error types for E2E testing

use thiserror::Error;

#[derive(Error, Debug)]
pub enum E2eError {
    #[error("Server failed to start: {0}")]
    ServerStartup(String),

    #[error("Server health check failed after {0} attempts")]
    ServerHealthCheck(usize),

    #[error("Playwright not found. Install with: npx playwright install")]
    PlaywrightNotFound,

    #[error("Playwright error: {0}")]
    Playwright(String),

    #[error("Test spec parse error: {0}")]
    SpecParse(String),

    #[error("Step failed: {step} - {reason}")]
    StepFailed { step: String, reason: String },

    #[error("Assertion failed: {0}")]
    AssertionFailed(String),

    #[error("Visual regression: {0}")]
    VisualRegression(String),

    #[error("Screenshot mismatch: {name} differs by {diff_percent:.2}% (threshold: {threshold:.2}%)")]
    ScreenshotMismatch {
        name: String,
        diff_percent: f64,
        threshold: f64,
    },

    #[error("Baseline not found: {0}")]
    BaselineNotFound(String),

    #[error("Timeout waiting for: {0}")]
    Timeout(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
}

pub type E2eResult<T> = Result<T, E2eError>;
