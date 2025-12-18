//! Playwright browser automation

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::mpsc;
use tracing::{debug, info, warn, error};

use crate::error::{E2eError, E2eResult};
use crate::spec::{TestStep, WaitState, AttributeAssertion};

/// Playwright browser handle
pub struct PlaywrightHandle {
    /// Base URL of the server
    base_url: String,
    
    /// Directory for screenshots
    screenshot_dir: PathBuf,
    
    /// Viewport dimensions
    viewport_width: u32,
    viewport_height: u32,
    
    /// Browser type
    browser: Browser,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum Browser {
    #[default]
    Chromium,
    Firefox,
    Webkit,
}

impl Browser {
    fn as_str(&self) -> &'static str {
        match self {
            Browser::Chromium => "chromium",
            Browser::Firefox => "firefox",
            Browser::Webkit => "webkit",
        }
    }
}

/// Result of executing a test step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub success: bool,
    pub step_name: String,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub screenshot_path: Option<PathBuf>,
}

impl PlaywrightHandle {
    /// Create a new Playwright handle
    pub fn new(config: PlaywrightConfig) -> E2eResult<Self> {
        // Verify playwright is installed
        Self::check_playwright_installed()?;
        
        // Create screenshot directory
        std::fs::create_dir_all(&config.screenshot_dir)?;
        
        Ok(Self {
            base_url: config.base_url,
            screenshot_dir: config.screenshot_dir,
            viewport_width: config.viewport_width,
            viewport_height: config.viewport_height,
            browser: config.browser,
        })
    }

    /// Check if Playwright is installed
    fn check_playwright_installed() -> E2eResult<()> {
        let output = Command::new("npx")
            .args(["playwright", "--version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match output {
            Ok(status) if status.success() => Ok(()),
            _ => Err(E2eError::PlaywrightNotFound),
        }
    }

    /// Execute a single test step
    pub async fn execute_step(&self, step: &TestStep) -> E2eResult<StepResult> {
        let start = std::time::Instant::now();
        let step_name = self.step_name(step);
        
        debug!("Executing step: {}", step_name);

        let result = match step {
            TestStep::Navigate { url, wait_for_selector } => {
                self.execute_navigate(url, wait_for_selector.as_deref()).await
            }
            TestStep::Click { selector, timeout_ms } => {
                self.execute_click(selector, *timeout_ms).await
            }
            TestStep::Fill { selector, value, clear_first } => {
                self.execute_fill(selector, value, *clear_first).await
            }
            TestStep::Type { selector, text, delay_ms } => {
                self.execute_type(selector, text, *delay_ms).await
            }
            TestStep::Press { selector, key } => {
                self.execute_press(selector.as_deref(), key).await
            }
            TestStep::Wait { selector, timeout_ms, state } => {
                self.execute_wait(selector, *timeout_ms, state).await
            }
            TestStep::Sleep { ms } => {
                tokio::time::sleep(Duration::from_millis(*ms)).await;
                Ok(None)
            }
            TestStep::Assert { selector, visible, text, text_contains, attribute, count } => {
                self.execute_assert(selector, *visible, text.as_deref(), text_contains.as_deref(), attribute.as_ref(), *count).await
            }
            TestStep::Screenshot { name, selector, full_page } => {
                self.execute_screenshot(name, selector.as_deref(), *full_page).await
            }
            TestStep::Hover { selector } => {
                self.execute_hover(selector).await
            }
            TestStep::Focus { selector } => {
                self.execute_focus(selector).await
            }
            TestStep::Select { selector, value } => {
                self.execute_select(selector, value).await
            }
            TestStep::Check { selector } => {
                self.execute_check(selector).await
            }
            TestStep::Uncheck { selector } => {
                self.execute_uncheck(selector).await
            }
            TestStep::Evaluate { script, expected } => {
                self.execute_evaluate(script, expected.as_ref()).await
            }
            TestStep::Log { message } => {
                info!("[TEST LOG] {}", message);
                Ok(None)
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(screenshot_path) => Ok(StepResult {
                success: true,
                step_name,
                duration_ms,
                error: None,
                screenshot_path,
            }),
            Err(e) => Ok(StepResult {
                success: false,
                step_name,
                duration_ms,
                error: Some(e.to_string()),
                screenshot_path: None,
            }),
        }
    }

    /// Generate a script name for a step
    fn step_name(&self, step: &TestStep) -> String {
        match step {
            TestStep::Navigate { url, .. } => format!("navigate:{}", url),
            TestStep::Click { selector, .. } => format!("click:{}", selector),
            TestStep::Fill { selector, .. } => format!("fill:{}", selector),
            TestStep::Type { selector, .. } => format!("type:{}", selector),
            TestStep::Press { key, .. } => format!("press:{}", key),
            TestStep::Wait { selector, .. } => format!("wait:{}", selector),
            TestStep::Sleep { ms } => format!("sleep:{}ms", ms),
            TestStep::Assert { selector, .. } => format!("assert:{}", selector),
            TestStep::Screenshot { name, .. } => format!("screenshot:{}", name),
            TestStep::Hover { selector } => format!("hover:{}", selector),
            TestStep::Focus { selector } => format!("focus:{}", selector),
            TestStep::Select { selector, .. } => format!("select:{}", selector),
            TestStep::Check { selector } => format!("check:{}", selector),
            TestStep::Uncheck { selector } => format!("uncheck:{}", selector),
            TestStep::Evaluate { .. } => "evaluate".to_string(),
            TestStep::Log { message } => format!("log:{}", &message[..message.len().min(30)]),
        }
    }

    /// Build the Playwright test script for a set of steps
    pub fn build_script(&self, steps: &[TestStep]) -> String {
        let mut script = String::new();
        
        // Header
        script.push_str(&format!(r#"
const {{ chromium, firefox, webkit }} = require('playwright');

(async () => {{
  const browser = await {browser}.launch({{ headless: true }});
  const context = await browser.newContext({{
    viewport: {{ width: {width}, height: {height} }}
  }});
  const page = await context.newPage();
  const baseUrl = '{base_url}';
  
  try {{
"#,
            browser = self.browser.as_str(),
            width = self.viewport_width,
            height = self.viewport_height,
            base_url = self.base_url,
        ));

        // Generate step code
        for (i, step) in steps.iter().enumerate() {
            script.push_str(&format!("\n    // Step {}: {}\n", i + 1, self.step_name(step)));
            script.push_str(&self.step_to_js(step, i));
        }

        // Footer
        script.push_str(r#"
    console.log(JSON.stringify({ success: true }));
  } catch (error) {
    console.error(JSON.stringify({ success: false, error: error.message, stack: error.stack }));
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
"#);

        script
    }

    /// Convert a step to JavaScript code
    fn step_to_js(&self, step: &TestStep, step_index: usize) -> String {
        match step {
            TestStep::Navigate { url, wait_for_selector } => {
                let wait = wait_for_selector.as_ref()
                    .map(|s| format!(r#"
    await page.waitForSelector('{}');"#, s))
                    .unwrap_or_default();
                format!(r#"    await page.goto(baseUrl + '{}');{}"#, url, wait)
            }
            TestStep::Click { selector, timeout_ms } => {
                let timeout = timeout_ms.unwrap_or(5000);
                format!(r#"    await page.click('{}', {{ timeout: {} }});"#, selector, timeout)
            }
            TestStep::Fill { selector, value, clear_first } => {
                if *clear_first {
                    format!(r#"    await page.fill('{}', '');
    await page.fill('{}', '{}');"#, selector, selector, value)
                } else {
                    format!(r#"    await page.fill('{}', '{}');"#, selector, value)
                }
            }
            TestStep::Type { selector, text, delay_ms } => {
                let delay = delay_ms.unwrap_or(50);
                format!(r#"    await page.type('{}', '{}', {{ delay: {} }});"#, selector, text, delay)
            }
            TestStep::Press { selector, key } => {
                match selector {
                    Some(sel) => format!(r#"    await page.locator('{}').press('{}');"#, sel, key),
                    None => format!(r#"    await page.keyboard.press('{}');"#, key),
                }
            }
            TestStep::Wait { selector, timeout_ms, state } => {
                let state_str = match state {
                    WaitState::Visible => "visible",
                    WaitState::Hidden => "hidden",
                    WaitState::Attached => "attached",
                    WaitState::Detached => "detached",
                };
                format!(r#"    await page.waitForSelector('{}', {{ state: '{}', timeout: {} }});"#, 
                    selector, state_str, timeout_ms)
            }
            TestStep::Sleep { ms } => {
                format!(r#"    await page.waitForTimeout({});"#, ms)
            }
            TestStep::Assert { selector, visible, text, text_contains, attribute, count } => {
                let mut assertions = Vec::new();
                
                if let Some(vis) = visible {
                    if *vis {
                        assertions.push(format!(
                            r#"    await expect(page.locator('{}')).toBeVisible();"#, selector));
                    } else {
                        assertions.push(format!(
                            r#"    await expect(page.locator('{}')).toBeHidden();"#, selector));
                    }
                }
                
                if let Some(t) = text {
                    assertions.push(format!(
                        r#"    await expect(page.locator('{}')).toHaveText('{}');"#, selector, t));
                }
                
                if let Some(tc) = text_contains {
                    assertions.push(format!(
                        r#"    await expect(page.locator('{}')).toContainText('{}');"#, selector, tc));
                }
                
                if let Some(attr) = attribute {
                    if let Some(val) = &attr.value {
                        assertions.push(format!(
                            r#"    await expect(page.locator('{}')).toHaveAttribute('{}', '{}');"#, 
                            selector, attr.name, val));
                    }
                }
                
                if let Some(c) = count {
                    assertions.push(format!(
                        r#"    await expect(page.locator('{}')).toHaveCount({});"#, selector, c));
                }
                
                assertions.join("\n")
            }
            TestStep::Screenshot { name, selector, full_page } => {
                let screenshot_path = self.screenshot_dir.join(format!("{}.png", name));
                let path_str = screenshot_path.to_string_lossy();
                
                if let Some(sel) = selector {
                    format!(r#"    await page.locator('{}').screenshot({{ path: '{}' }});"#, sel, path_str)
                } else {
                    format!(r#"    await page.screenshot({{ path: '{}', fullPage: {} }});"#, path_str, full_page)
                }
            }
            TestStep::Hover { selector } => {
                format!(r#"    await page.hover('{}');"#, selector)
            }
            TestStep::Focus { selector } => {
                format!(r#"    await page.focus('{}');"#, selector)
            }
            TestStep::Select { selector, value } => {
                format!(r#"    await page.selectOption('{}', '{}');"#, selector, value)
            }
            TestStep::Check { selector } => {
                format!(r#"    await page.check('{}');"#, selector)
            }
            TestStep::Uncheck { selector } => {
                format!(r#"    await page.uncheck('{}');"#, selector)
            }
            TestStep::Evaluate { script, expected } => {
                format!(r#"    const result_{} = await page.evaluate(() => {{ {} }});"#, step_index, script)
            }
            TestStep::Log { message } => {
                format!(r#"    console.log('[TEST] {}');"#, message.replace("'", "\\'"))
            }
        }
    }

    /// Execute the full script via Playwright
    pub async fn run_script(&self, script: &str) -> E2eResult<()> {
        // Write script to temp file
        let temp_dir = tempfile::tempdir()?;
        let script_path = temp_dir.path().join("test.js");
        std::fs::write(&script_path, script)?;

        debug!("Running Playwright script: {}", script_path.display());

        // Run with node
        let output = TokioCommand::new("node")
            .arg(&script_path)
            .current_dir(temp_dir.path())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(E2eError::Playwright(format!(
                "Script failed:\nstdout: {}\nstderr: {}",
                stdout, stderr
            )));
        }

        Ok(())
    }

    // Individual step execution methods (simplified - these call run_script internally)
    async fn execute_navigate(&self, url: &str, wait_for: Option<&str>) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Navigate { 
            url: url.to_string(), 
            wait_for_selector: wait_for.map(String::from) 
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_click(&self, selector: &str, timeout_ms: Option<u64>) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Click { 
            selector: selector.to_string(), 
            timeout_ms 
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_fill(&self, selector: &str, value: &str, clear_first: bool) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Fill { 
            selector: selector.to_string(), 
            value: value.to_string(), 
            clear_first 
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_type(&self, selector: &str, text: &str, delay_ms: Option<u64>) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Type { 
            selector: selector.to_string(), 
            text: text.to_string(), 
            delay_ms 
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_press(&self, selector: Option<&str>, key: &str) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Press { 
            selector: selector.map(String::from), 
            key: key.to_string() 
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_wait(&self, selector: &str, timeout_ms: u64, state: &WaitState) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Wait { 
            selector: selector.to_string(), 
            timeout_ms, 
            state: state.clone() 
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_assert(
        &self,
        selector: &str,
        visible: Option<bool>,
        text: Option<&str>,
        text_contains: Option<&str>,
        attribute: Option<&AttributeAssertion>,
        count: Option<usize>,
    ) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Assert { 
            selector: selector.to_string(), 
            visible,
            text: text.map(String::from),
            text_contains: text_contains.map(String::from),
            attribute: attribute.cloned(),
            count,
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_screenshot(&self, name: &str, selector: Option<&str>, full_page: bool) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Screenshot { 
            name: name.to_string(), 
            selector: selector.map(String::from), 
            full_page 
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        
        let path = self.screenshot_dir.join(format!("{}.png", name));
        Ok(Some(path))
    }

    async fn execute_hover(&self, selector: &str) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Hover { selector: selector.to_string() };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_focus(&self, selector: &str) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Focus { selector: selector.to_string() };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_select(&self, selector: &str, value: &str) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Select { 
            selector: selector.to_string(), 
            value: value.to_string() 
        };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_check(&self, selector: &str) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Check { selector: selector.to_string() };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_uncheck(&self, selector: &str) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Uncheck { selector: selector.to_string() };
        let script = self.build_script(&[step]);
        self.run_script(&script).await?;
        Ok(None)
    }

    async fn execute_evaluate(&self, script: &str, expected: Option<&serde_json::Value>) -> E2eResult<Option<PathBuf>> {
        let step = TestStep::Evaluate { 
            script: script.to_string(), 
            expected: expected.cloned() 
        };
        let test_script = self.build_script(&[step]);
        self.run_script(&test_script).await?;
        Ok(None)
    }
}

/// Configuration for Playwright
#[derive(Debug, Clone)]
pub struct PlaywrightConfig {
    pub base_url: String,
    pub screenshot_dir: PathBuf,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub browser: Browser,
    pub headless: bool,
}

impl Default for PlaywrightConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080".to_string(),
            screenshot_dir: PathBuf::from("test-results/screenshots"),
            viewport_width: 1280,
            viewport_height: 720,
            browser: Browser::Chromium,
            headless: true,
        }
    }
}
