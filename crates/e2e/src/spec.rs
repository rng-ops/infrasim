//! Declarative YAML test specification

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{E2eError, E2eResult};

/// A complete test specification parsed from YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSpec {
    /// Unique name for this test
    pub name: String,

    /// Human-readable description
    #[serde(default)]
    pub description: String,

    /// Tags for filtering tests
    #[serde(default)]
    pub tags: Vec<String>,

    /// Viewport size for the browser
    #[serde(default = "default_viewport")]
    pub viewport: Viewport,

    /// Steps to execute in order
    pub steps: Vec<TestStep>,

    /// Whether this test includes visual regression
    #[serde(default)]
    pub visual_regression: bool,

    /// Threshold for visual diff (0.0 - 100.0 percent)
    #[serde(default = "default_threshold")]
    pub visual_threshold: f64,
}

fn default_viewport() -> Viewport {
    Viewport { width: 1280, height: 720 }
}

fn default_threshold() -> f64 {
    0.5 // 0.5% pixel difference allowed by default
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

/// A single step in a test
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TestStep {
    /// Navigate to a URL (relative to base)
    Navigate {
        url: String,
        #[serde(default)]
        wait_for_selector: Option<String>,
    },

    /// Click an element
    Click {
        selector: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },

    /// Fill an input field
    Fill {
        selector: String,
        value: String,
        #[serde(default)]
        clear_first: bool,
    },

    /// Type text with keyboard simulation
    Type {
        selector: String,
        text: String,
        #[serde(default)]
        delay_ms: Option<u64>,
    },

    /// Press a key
    Press {
        selector: Option<String>,
        key: String,
    },

    /// Wait for an element to appear
    Wait {
        selector: String,
        #[serde(default = "default_wait_timeout")]
        timeout_ms: u64,
        #[serde(default)]
        state: WaitState,
    },

    /// Wait for a fixed amount of time (use sparingly)
    Sleep {
        ms: u64,
    },

    /// Assert something about an element
    Assert {
        selector: String,
        #[serde(default)]
        visible: Option<bool>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        text_contains: Option<String>,
        #[serde(default)]
        attribute: Option<AttributeAssertion>,
        #[serde(default)]
        count: Option<usize>,
    },

    /// Take a screenshot
    Screenshot {
        name: String,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        full_page: bool,
    },

    /// Hover over an element
    Hover {
        selector: String,
    },

    /// Focus an element
    Focus {
        selector: String,
    },

    /// Select an option from a dropdown
    Select {
        selector: String,
        value: String,
    },

    /// Check a checkbox
    Check {
        selector: String,
    },

    /// Uncheck a checkbox
    Uncheck {
        selector: String,
    },

    /// Execute custom JavaScript
    Evaluate {
        script: String,
        #[serde(default)]
        expected: Option<serde_json::Value>,
    },

    /// Log a message (for debugging)
    Log {
        message: String,
    },
}

fn default_wait_timeout() -> u64 {
    5000 // 5 seconds default
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitState {
    #[default]
    Visible,
    Hidden,
    Attached,
    Detached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeAssertion {
    pub name: String,
    pub value: Option<String>,
    #[serde(default)]
    pub contains: Option<String>,
}

impl TestSpec {
    /// Parse a test spec from YAML string
    pub fn from_yaml(yaml: &str) -> E2eResult<Self> {
        serde_yaml::from_str(yaml).map_err(E2eError::from)
    }

    /// Parse a test spec from a YAML file
    pub fn from_file(path: &Path) -> E2eResult<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Load all test specs from a directory
    pub fn load_all(dir: &Path) -> E2eResult<Vec<Self>> {
        let mut specs = Vec::new();
        
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "yaml" || ext == "yml")
                    .unwrap_or(false)
            })
        {
            let spec = Self::from_file(entry.path())?;
            specs.push(spec);
        }
        
        Ok(specs)
    }

    /// Filter specs by tag
    pub fn filter_by_tag<'a>(specs: &'a [Self], tag: &str) -> Vec<&'a Self> {
        specs.iter().filter(|s| s.tags.contains(&tag.to_string())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_spec() {
        let yaml = r#"
name: login-flow
description: Test the TOTP login flow
tags:
  - auth
  - smoke
steps:
  - action: navigate
    url: /login
    wait_for_selector: '[data-testid="login-page"]'
  - action: fill
    selector: '[data-testid="login-identifier-input"]'
    value: testuser
  - action: screenshot
    name: login-form
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        assert_eq!(spec.name, "login-flow");
        assert_eq!(spec.steps.len(), 3);
    }

    #[test]
    fn test_parse_visual_regression_spec() {
        let yaml = r#"
name: dashboard-visual
description: Visual regression test for dashboard
visual_regression: true
visual_threshold: 1.0
viewport:
  width: 1920
  height: 1080
steps:
  - action: navigate
    url: /
  - action: wait
    selector: '[data-testid="app-shell"]'
  - action: screenshot
    name: dashboard-full
    full_page: true
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        assert!(spec.visual_regression);
        assert_eq!(spec.visual_threshold, 1.0);
        assert_eq!(spec.viewport.width, 1920);
    }
}
