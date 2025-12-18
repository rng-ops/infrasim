//! Visual regression testing with screenshot comparison

use std::path::{Path, PathBuf};
use image::{GenericImageView, Pixel, RgbaImage};
use sha2::{Sha256, Digest};
use tracing::{debug, info, warn};

use crate::error::{E2eError, E2eResult};

/// Result of a visual comparison
#[derive(Debug, Clone)]
pub struct VisualDiff {
    /// Whether the images match (within threshold)
    pub matches: bool,
    
    /// Percentage of pixels that differ
    pub diff_percent: f64,
    
    /// Number of different pixels
    pub diff_pixels: u64,
    
    /// Total pixels compared
    pub total_pixels: u64,
    
    /// Path to the diff image (if generated)
    pub diff_image_path: Option<PathBuf>,
    
    /// Hash of the actual screenshot
    pub actual_hash: String,
    
    /// Hash of the baseline screenshot
    pub baseline_hash: String,
}

/// Visual regression testing utilities
pub struct VisualTester {
    /// Directory containing baseline screenshots
    baseline_dir: PathBuf,
    
    /// Directory for actual screenshots
    actual_dir: PathBuf,
    
    /// Directory for diff images
    diff_dir: PathBuf,
    
    /// Default threshold (0.0 - 100.0 percent)
    threshold: f64,
    
    /// Whether to auto-update baselines when missing
    auto_update: bool,
}

impl VisualTester {
    /// Create a new visual tester
    pub fn new(config: VisualConfig) -> E2eResult<Self> {
        std::fs::create_dir_all(&config.baseline_dir)?;
        std::fs::create_dir_all(&config.actual_dir)?;
        std::fs::create_dir_all(&config.diff_dir)?;
        
        Ok(Self {
            baseline_dir: config.baseline_dir,
            actual_dir: config.actual_dir,
            diff_dir: config.diff_dir,
            threshold: config.threshold,
            auto_update: config.auto_update,
        })
    }

    /// Compare a screenshot against its baseline
    pub fn compare(&self, name: &str, threshold: Option<f64>) -> E2eResult<VisualDiff> {
        let threshold = threshold.unwrap_or(self.threshold);
        
        let actual_path = self.actual_dir.join(format!("{}.png", name));
        let baseline_path = self.baseline_dir.join(format!("{}.png", name));
        
        // Check actual exists
        if !actual_path.exists() {
            return Err(E2eError::VisualRegression(format!(
                "Actual screenshot not found: {}", actual_path.display()
            )));
        }

        // Check baseline exists
        if !baseline_path.exists() {
            if self.auto_update {
                info!("Creating baseline for '{}' (auto-update enabled)", name);
                std::fs::copy(&actual_path, &baseline_path)?;
                
                let actual_hash = self.hash_file(&actual_path)?;
                return Ok(VisualDiff {
                    matches: true,
                    diff_percent: 0.0,
                    diff_pixels: 0,
                    total_pixels: 0,
                    diff_image_path: None,
                    actual_hash: actual_hash.clone(),
                    baseline_hash: actual_hash,
                });
            } else {
                return Err(E2eError::BaselineNotFound(baseline_path.to_string_lossy().to_string()));
            }
        }

        // Load images
        let actual_img = image::open(&actual_path)?;
        let baseline_img = image::open(&baseline_path)?;
        
        // Hash files
        let actual_hash = self.hash_file(&actual_path)?;
        let baseline_hash = self.hash_file(&baseline_path)?;
        
        // Quick hash comparison
        if actual_hash == baseline_hash {
            debug!("Screenshots match exactly (same hash)");
            return Ok(VisualDiff {
                matches: true,
                diff_percent: 0.0,
                diff_pixels: 0,
                total_pixels: (actual_img.width() * actual_img.height()) as u64,
                diff_image_path: None,
                actual_hash,
                baseline_hash,
            });
        }

        // Check dimensions
        if actual_img.dimensions() != baseline_img.dimensions() {
            warn!(
                "Screenshot dimensions differ: actual {:?} vs baseline {:?}",
                actual_img.dimensions(),
                baseline_img.dimensions()
            );
            
            // Still try to compare overlapping region
        }

        // Pixel-by-pixel comparison
        let (width, height) = actual_img.dimensions();
        let baseline_rgba = baseline_img.to_rgba8();
        let actual_rgba = actual_img.to_rgba8();
        
        let mut diff_img = RgbaImage::new(width, height);
        let mut diff_pixels = 0u64;
        let total_pixels = (width as u64) * (height as u64);

        for y in 0..height.min(baseline_img.height()) {
            for x in 0..width.min(baseline_img.width()) {
                let actual_pixel = actual_rgba.get_pixel(x, y);
                let baseline_pixel = baseline_rgba.get_pixel(x, y);
                
                if self.pixels_differ(actual_pixel, baseline_pixel) {
                    diff_pixels += 1;
                    // Mark diff pixels in red
                    diff_img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
                } else {
                    // Keep original but dim it
                    let channels = actual_pixel.channels();
                    diff_img.put_pixel(x, y, image::Rgba([
                        channels[0] / 2,
                        channels[1] / 2,
                        channels[2] / 2,
                        128,
                    ]));
                }
            }
        }

        let diff_percent = (diff_pixels as f64 / total_pixels as f64) * 100.0;
        let matches = diff_percent <= threshold;

        // Save diff image if there are differences
        let diff_image_path = if diff_pixels > 0 {
            let path = self.diff_dir.join(format!("{}-diff.png", name));
            diff_img.save(&path)?;
            Some(path)
        } else {
            None
        };

        if !matches {
            warn!(
                "Visual regression detected in '{}': {:.2}% pixels differ (threshold: {:.2}%)",
                name, diff_percent, threshold
            );
        }

        Ok(VisualDiff {
            matches,
            diff_percent,
            diff_pixels,
            total_pixels,
            diff_image_path,
            actual_hash,
            baseline_hash,
        })
    }

    /// Update the baseline with the actual screenshot
    pub fn update_baseline(&self, name: &str) -> E2eResult<()> {
        let actual_path = self.actual_dir.join(format!("{}.png", name));
        let baseline_path = self.baseline_dir.join(format!("{}.png", name));
        
        if !actual_path.exists() {
            return Err(E2eError::VisualRegression(format!(
                "Cannot update baseline: actual screenshot not found: {}", 
                actual_path.display()
            )));
        }

        std::fs::copy(&actual_path, &baseline_path)?;
        info!("Updated baseline for '{}'", name);
        
        Ok(())
    }

    /// Check if two pixels differ significantly
    fn pixels_differ(&self, a: &image::Rgba<u8>, b: &image::Rgba<u8>) -> bool {
        let a_channels = a.channels();
        let b_channels = b.channels();
        
        // Allow small color differences (anti-aliasing, compression)
        const TOLERANCE: i32 = 5;
        
        for i in 0..4 {
            let diff = (a_channels[i] as i32 - b_channels[i] as i32).abs();
            if diff > TOLERANCE {
                return true;
            }
        }
        
        false
    }

    /// Hash a file using SHA256
    fn hash_file(&self, path: &Path) -> E2eResult<String> {
        let data = std::fs::read(path)?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        Ok(hex::encode(hasher.finalize()))
    }

    /// List all baselines
    pub fn list_baselines(&self) -> E2eResult<Vec<String>> {
        let mut baselines = Vec::new();
        
        for entry in std::fs::read_dir(&self.baseline_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().map(|e| e == "png").unwrap_or(false) {
                if let Some(name) = path.file_stem() {
                    baselines.push(name.to_string_lossy().to_string());
                }
            }
        }
        
        Ok(baselines)
    }

    /// Clean up old diff images
    pub fn clean_diffs(&self) -> E2eResult<()> {
        for entry in std::fs::read_dir(&self.diff_dir)? {
            let entry = entry?;
            std::fs::remove_file(entry.path())?;
        }
        Ok(())
    }
}

/// Configuration for visual testing
#[derive(Debug, Clone)]
pub struct VisualConfig {
    pub baseline_dir: PathBuf,
    pub actual_dir: PathBuf,
    pub diff_dir: PathBuf,
    pub threshold: f64,
    pub auto_update: bool,
}

impl Default for VisualConfig {
    fn default() -> Self {
        Self {
            baseline_dir: PathBuf::from("test-results/baselines"),
            actual_dir: PathBuf::from("test-results/screenshots"),
            diff_dir: PathBuf::from("test-results/diffs"),
            threshold: 0.5,
            auto_update: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visual_config_default() {
        let config = VisualConfig::default();
        assert_eq!(config.threshold, 0.5);
        assert!(!config.auto_update);
    }
}
