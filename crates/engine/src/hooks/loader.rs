//! Hook discovery and loading
//!
//! Loads hook definitions from the .guisu/hooks directory structure.

use super::config::{Hook, HookCollections, HookMode};
use guisu_core::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Discover and load hooks from the hooks directory
pub struct HookLoader {
    hooks_dir: PathBuf,
}

impl HookLoader {
    /// Create a new hook loader for the given source directory
    pub fn new(source_dir: &Path) -> Self {
        Self {
            hooks_dir: source_dir.join(".guisu/hooks"),
        }
    }

    /// Check if hooks directory exists
    pub fn exists(&self) -> bool {
        self.hooks_dir.exists()
    }

    /// Load all hooks from the hooks directory
    pub fn load(&self) -> Result<HookCollections> {
        if !self.hooks_dir.exists() {
            tracing::debug!(
                "Hooks directory does not exist: {}",
                self.hooks_dir.display()
            );
            return Ok(HookCollections::default());
        }

        let mut collections = HookCollections::default();

        // Load pre hooks
        let pre_dir = self.hooks_dir.join("pre");
        if pre_dir.exists() {
            collections.pre = self
                .load_hooks_from_dir(&pre_dir)
                .map_err(|e| Error::HookConfig(format!("Failed to load pre hooks: {}", e)))?;
        }

        // Load post hooks
        let post_dir = self.hooks_dir.join("post");
        if post_dir.exists() {
            collections.post = self
                .load_hooks_from_dir(&post_dir)
                .map_err(|e| Error::HookConfig(format!("Failed to load post hooks: {}", e)))?;
        }

        Ok(collections)
    }

    /// Load hooks from a specific directory (pre or post)
    fn load_hooks_from_dir(&self, dir: &Path) -> Result<Vec<Hook>> {
        use rayon::prelude::*;

        // First pass: Collect and sort file paths (must be sequential)
        let mut file_paths: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| {
                Error::HookConfig(format!("Failed to read directory {}: {}", dir.display(), e))
            })?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.path())
            .filter(|path| {
                // Skip hidden files and editor backups
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    !file_name.starts_with('.')
                        && !file_name.ends_with('~')
                        && !file_name.ends_with(".swp")
                } else {
                    false
                }
            })
            .collect();

        // Sort by filename for consistent ordering (important for numeric prefixes)
        file_paths.sort();

        // Second pass: Parallel file loading and parsing
        // Each file gets an order value based on its position (0, 10, 20, 30...)
        let hooks_result: Result<Vec<Vec<Hook>>> = file_paths
            .par_iter()
            .enumerate()
            .map(|(idx, path)| {
                let base_order = (idx * 10) as i32;
                tracing::debug!(
                    "Loading hook file: {} (order: {})",
                    path.display(),
                    base_order
                );
                self.load_hook_file(path, base_order)
            })
            .collect();

        // Flatten results into single vector
        let hooks = hooks_result?.into_iter().flatten().collect();

        Ok(hooks)
    }

    /// Load hooks from a single file
    fn load_hook_file(&self, path: &Path, base_order: i32) -> Result<Vec<Hook>> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Get the extension
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Configuration files - parse and load hooks
        if ext == "toml" {
            return self.load_toml_hooks(path, base_order);
        }

        // Check if file is executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(path) {
                let permissions = metadata.permissions();
                if permissions.mode() & 0o111 != 0 {
                    // Read script content for diffing
                    let script_content = fs::read_to_string(path).ok();

                    // File is executable - create hook
                    let hook = Hook {
                        name: file_name.to_string(),
                        order: base_order,
                        platforms: vec![],
                        cmd: Some(path.to_string_lossy().to_string()),
                        script: None,
                        script_content,
                        env: Default::default(),
                        failfast: true,
                        mode: HookMode::default(),
                        timeout: 0, // No timeout by default
                    };
                    return Ok(vec![hook]);
                }
            }
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, skip executable check
            tracing::warn!(
                "Executable check not supported on this platform: {}",
                path.display()
            );
        }

        tracing::warn!("Skipping non-executable file: {}", path.display());
        Ok(vec![])
    }

    /// Load hooks from TOML file
    fn load_toml_hooks(&self, path: &Path, _base_order: i32) -> Result<Vec<Hook>> {
        let content = fs::read_to_string(path).map_err(|e| {
            Error::HookConfig(format!(
                "Failed to read TOML file {}: {}",
                path.display(),
                e
            ))
        })?;

        // Try to parse as array of hooks first
        if let Ok(mut hooks) = toml::from_str::<Vec<Hook>>(&content) {
            // Resolve script paths relative to hook file directory
            for hook in &mut hooks {
                self.resolve_script_path(hook, path)?;
            }
            return Ok(hooks);
        }

        // Try to parse as single hook
        if let Ok(mut hook) = toml::from_str::<Hook>(&content) {
            // Resolve script path relative to hook file directory
            self.resolve_script_path(&mut hook, path)?;
            return Ok(vec![hook]);
        }

        Err(Error::HookConfig(format!(
            "Failed to parse TOML hooks from: {}",
            path.display()
        )))
    }

    /// Resolve script path relative to hook file directory
    ///
    /// This function supports automatic .j2 template detection:
    /// - If script = "script.sh.j2", uses it directly as a template
    /// - If script = "script.sh" and "script.sh.j2" exists, uses the template version
    /// - Otherwise, uses the specified path as-is
    fn resolve_script_path(&self, hook: &mut Hook, hook_file_path: &Path) -> Result<()> {
        if let Some(script) = &hook.script {
            // Skip absolute paths
            if script.starts_with('/') {
                return Ok(());
            }

            // Get hook file directory
            let hook_dir = hook_file_path.parent().ok_or_else(|| {
                Error::HookConfig(format!(
                    "Cannot get parent directory of hook file: {}",
                    hook_file_path.display()
                ))
            })?;

            // Resolve script path relative to hook directory
            let script_abs = hook_dir.join(script);

            // Auto-detect .j2 template version
            let final_script_abs = if script.ends_with(".j2") {
                // Explicitly specified as template
                script_abs
            } else {
                // Check if .j2 version exists
                let template_version = hook_dir.join(format!("{}.j2", script));
                if template_version.exists() {
                    tracing::debug!(
                        "Auto-detected template version: {} -> {}",
                        script,
                        template_version.display()
                    );
                    template_version
                } else {
                    // Use original path
                    script_abs
                }
            };

            // Get source directory (.guisu/hooks -> .guisu -> source_dir)
            let source_dir = self
                .hooks_dir
                .parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| {
                    Error::HookConfig(format!(
                        "Cannot determine source directory from hooks dir: {}",
                        self.hooks_dir.display()
                    ))
                })?;

            // Convert to relative path from source_dir
            let script_rel = final_script_abs.strip_prefix(source_dir).map_err(|_| {
                Error::HookConfig(format!(
                    "Script path is outside source directory: {}",
                    final_script_abs.display()
                ))
            })?;

            hook.script = Some(script_rel.display().to_string());

            // Read and store script content for diffing
            if final_script_abs.exists() {
                if let Ok(content) = fs::read_to_string(&final_script_abs) {
                    hook.script_content = Some(content);
                } else {
                    tracing::warn!(
                        "Failed to read script content for diffing: {}",
                        final_script_abs.display()
                    );
                }
            }
        }

        Ok(())
    }
}
