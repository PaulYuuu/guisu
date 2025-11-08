//! Template engine implementation
//!
//! The engine wraps minijinja and provides template rendering with custom functions.

use crate::context::TemplateContext;
use crate::functions;
use crate::{Error, Result};
use guisu_crypto::Identity;
use minijinja::Environment;
use std::path::PathBuf;
use std::sync::Arc;

/// Template engine for rendering templates
pub struct TemplateEngine {
    /// The minijinja environment
    env: Environment<'static>,
}

impl TemplateEngine {
    /// Create a new template engine without decryption support
    pub fn new() -> Self {
        Self::with_identities(Vec::new())
    }

    /// Create a template engine with identities for decryption
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use guisu_template::TemplateEngine;
    /// use guisu_crypto::load_identities;
    /// use std::path::PathBuf;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let identities = load_identities(&PathBuf::from("~/.config/guisu/key.txt"), false)?;
    /// let engine = TemplateEngine::with_identities(identities);
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_identities(identities: Vec<Identity>) -> Self {
        Self::with_identities_and_template_dir(identities, None)
    }

    /// Create a template engine with identities and a template directory
    ///
    /// The template directory supports platform-specific templates:
    /// - Templates in `templates/darwin/` are used on macOS
    /// - Templates in `templates/linux/` are used on Linux
    /// - Templates in `templates/` are used as fallback
    ///
    /// When using `{% include "Brewfile" %}`, the engine searches:
    /// 1. `templates/{platform}/Brewfile.j2`
    /// 2. `templates/{platform}/Brewfile`
    /// 3. `templates/Brewfile.j2`
    /// 4. `templates/Brewfile`
    ///
    /// Templates ending with `.j2` support nested Jinja2 rendering.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use guisu_template::TemplateEngine;
    /// use guisu_crypto::load_identities;
    /// use std::path::PathBuf;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let identities = load_identities(&PathBuf::from("~/.config/guisu/key.txt"), false)?;
    /// let template_dir = PathBuf::from("/path/to/source/.guisu/templates");
    /// let engine = TemplateEngine::with_identities_and_template_dir(identities, Some(template_dir));
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_identities_and_template_dir(
        identities: Vec<Identity>,
        template_dir: Option<PathBuf>,
    ) -> Self {
        Self::with_identities_arc_and_template_dir(Arc::new(identities), template_dir)
    }

    /// Create a template engine with Arc-wrapped identities and optional template directory
    ///
    /// This version accepts Arc-wrapped identities to avoid cloning when the identities
    /// are already wrapped in Arc.
    pub fn with_identities_arc_and_template_dir(
        identities: Arc<Vec<Identity>>,
        template_dir: Option<PathBuf>,
    ) -> Self {
        Self::with_identities_arc_template_dir_and_bitwarden_provider(
            identities,
            template_dir,
            "bw", // default provider
        )
    }

    /// Create a template engine with all configuration options
    ///
    /// This is the most complete constructor that accepts:
    /// - Identities for encryption/decryption
    /// - Template directory for include/includeTemplate
    /// - Bitwarden provider selection ("bw" or "rbw")
    pub fn with_identities_arc_template_dir_and_bitwarden_provider(
        identities: Arc<Vec<Identity>>,
        template_dir: Option<PathBuf>,
        bitwarden_provider: &str,
    ) -> Self {
        let mut env = Environment::new();

        // Enable Jinja2 standard whitespace control
        // trim_blocks: automatically remove newlines after block tags
        // lstrip_blocks: automatically strip leading whitespace from block lines
        // keep_trailing_newline: ensure files always end with a newline
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);
        env.set_keep_trailing_newline(true);

        // Register custom functions
        env.add_function("env", functions::env);
        env.add_function("os", functions::os);
        env.add_function("arch", functions::arch);
        env.add_function("hostname", functions::hostname);
        env.add_function("username", functions::username);
        env.add_function("home_dir", functions::home_dir);
        env.add_function("joinPath", functions::join_path);
        env.add_function("lookPath", functions::look_path);
        env.add_function("include", functions::include);
        env.add_function("includeTemplate", functions::include_template);

        // Register Bitwarden functions with provider closure
        #[cfg(any(feature = "bw", feature = "rbw"))]
        {
            let provider = bitwarden_provider.to_string();

            let provider_clone = provider.clone();
            env.add_function("bitwarden", move |args: &[minijinja::Value]| {
                functions::bitwarden(args, &provider_clone)
            });

            let provider_clone = provider.clone();
            env.add_function("bitwardenFields", move |args: &[minijinja::Value]| {
                functions::bitwarden_fields(args, &provider_clone)
            });

            #[cfg(feature = "bw")]
            {
                env.add_function("bitwardenAttachment", move |args: &[minijinja::Value]| {
                    functions::bitwarden_attachment(args, &provider)
                });
            }
        }

        #[cfg(feature = "bws")]
        env.add_function("bitwardenSecrets", functions::bitwarden_secrets);

        // Register filters
        env.add_filter("quote", functions::quote);
        env.add_filter("toJson", functions::to_json);
        env.add_filter("fromJson", functions::from_json);
        env.add_filter("toToml", functions::to_toml);
        env.add_filter("fromToml", functions::from_toml);
        env.add_filter("trim", functions::trim);
        env.add_filter("trimStart", functions::trim_start);
        env.add_filter("trimEnd", functions::trim_end);

        // Register string processing functions
        env.add_function("regexMatch", functions::regex_match);
        env.add_function("regexReplaceAll", functions::regex_replace_all);
        env.add_function("split", functions::split);
        env.add_function("join", functions::join);

        // Register decrypt filter with captured identities
        let identities_clone = Arc::clone(&identities);
        env.add_filter("decrypt", move |value: &str| {
            functions::decrypt(value, &identities_clone)
        });

        // Register encrypt filter with captured identities
        let identities_clone = Arc::clone(&identities);
        env.add_filter("encrypt", move |value: &str| {
            functions::encrypt(value, &identities_clone)
        });

        // Set up smart template loader with platform support
        if let Some(template_dir) = template_dir
            && template_dir.exists()
            && template_dir.is_dir()
        {
            // Detect current platform
            #[cfg(target_os = "linux")]
            let platform = "linux";
            #[cfg(target_os = "macos")]
            let platform = "darwin";
            #[cfg(target_os = "windows")]
            let platform = "windows";
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            let platform = "unknown";

            let platform = platform.to_string();

            env.set_loader(move |name| {
                // Search order for template "Brewfile":
                // 1. templates/darwin/Brewfile.j2
                // 2. templates/darwin/Brewfile
                // 3. templates/Brewfile.j2
                // 4. templates/Brewfile

                let candidates = vec![
                    template_dir.join(&platform).join(format!("{}.j2", name)),
                    template_dir.join(&platform).join(name),
                    template_dir.join(format!("{}.j2", name)),
                    template_dir.join(name),
                ];

                for path in candidates {
                    if path.exists() {
                        return match std::fs::read_to_string(&path) {
                            Ok(content) => Ok(Some(content)),
                            Err(e) => Err(minijinja::Error::new(
                                minijinja::ErrorKind::InvalidOperation,
                                format!("Failed to read template '{}': {}", name, e),
                            )),
                        };
                    }
                }

                // Template not found
                Ok(None)
            });
        }

        Self { env }
    }

    /// Render a template string with the given context
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_template::{TemplateEngine, TemplateContext};
    ///
    /// let engine = TemplateEngine::new();
    /// let context = TemplateContext::new();
    ///
    /// let template = "Hello {{ username }}!";
    /// let result = engine.render_str(template, &context).unwrap();
    /// println!("{}", result);
    /// ```
    pub fn render_str(&self, template: &str, context: &TemplateContext) -> Result<String> {
        self.env.render_str(template, context).map_err(Error::from)
    }

    /// Render a template string with a specific name for better error messages
    ///
    /// This method is preferred over `render_str` when you have a file path or
    /// meaningful name to associate with the template. Error messages will include
    /// this name instead of the generic `<string>`.
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_template::{TemplateEngine, TemplateContext};
    ///
    /// let engine = TemplateEngine::new();
    /// let context = TemplateContext::new();
    ///
    /// let template = "Hello {{ username }}!";
    /// let result = engine.render_named_str("greeting.txt", template, &context).unwrap();
    /// println!("{}", result);
    /// ```
    pub fn render_named_str(
        &self,
        name: &str,
        template: &str,
        context: &TemplateContext,
    ) -> Result<String> {
        self.env
            .render_named_str(name, template, context)
            .map_err(Error::from)
    }

    /// Render template content (bytes) with the given context
    ///
    /// This is useful for rendering template files that may contain binary data
    /// in certain sections, though the template syntax itself must be valid UTF-8.
    pub fn render(&self, template: &[u8], context: &TemplateContext) -> Result<Vec<u8>> {
        let template_str = std::str::from_utf8(template)
            .map_err(|e| Error::Syntax(format!("Template is not valid UTF-8: {}", e)))?;

        let rendered = self.render_str(template_str, context)?;
        Ok(rendered.into_bytes())
    }

    /// Check if a string contains template syntax
    ///
    /// This is a simple heuristic check that looks for Jinja2-style syntax.
    pub fn is_template(content: &str) -> bool {
        content.contains("{{") || content.contains("{%") || content.contains("{#")
    }

    /// Get a reference to the underlying minijinja environment
    ///
    /// This allows for advanced customization if needed.
    pub fn env(&self) -> &Environment<'static> {
        &self.env
    }

    /// Get a mutable reference to the underlying minijinja environment
    ///
    /// This allows for advanced customization if needed.
    pub fn env_mut(&mut self) -> &mut Environment<'static> {
        &mut self.env
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

// Implement TemplateRenderer trait for TemplateEngine
impl guisu_core::TemplateRenderer for TemplateEngine {
    fn render_str(
        &self,
        template: &str,
        context: &serde_json::Value,
    ) -> guisu_core::Result<String> {
        self.env
            .render_str(template, context)
            .map_err(|e| guisu_core::Error::Message(Error::from(e).to_string()))
    }

    fn render_named_str(
        &self,
        name: &str,
        template: &str,
        context: &serde_json::Value,
    ) -> guisu_core::Result<String> {
        self.env
            .render_named_str(name, template, context)
            .map_err(|e| guisu_core::Error::Message(Error::from(e).to_string()))
    }
}
