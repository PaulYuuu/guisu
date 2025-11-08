//! Template adapter that implements the TemplateRenderer trait from engine

use crate::content::TemplateRenderer;
use guisu_template::{TemplateContext, TemplateEngine};
use std::sync::Arc;
use thiserror::Error;

/// Error type for template adapter
#[derive(Error, Debug)]
pub enum TemplateError {
    #[error(transparent)]
    Template(#[from] guisu_template::Error),

    #[error("Failed to convert context: {0}")]
    ContextConversion(String),
}

/// Adapter that wraps TemplateEngine to implement engine::content::TemplateRenderer
pub struct TemplateRendererAdapter {
    engine: Arc<TemplateEngine>,
}

impl TemplateRendererAdapter {
    /// Create a new template adapter
    pub fn new(engine: TemplateEngine) -> Self {
        Self {
            engine: Arc::new(engine),
        }
    }

    /// Get a reference to the underlying TemplateEngine
    pub fn inner(&self) -> &TemplateEngine {
        &self.engine
    }
}

impl TemplateRenderer for TemplateRendererAdapter {
    type Error = TemplateError;

    fn render(&self, template: &str, context: &serde_json::Value) -> Result<String, Self::Error> {
        // Convert serde_json::Value to TemplateContext
        let variables = if let serde_json::Value::Object(map) = context {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            return Err(TemplateError::ContextConversion(
                "Context must be a JSON object".to_string(),
            ));
        };

        let template_context = TemplateContext::new().with_variables(variables);

        self.engine
            .render_str(template, &template_context)
            .map_err(Into::into)
    }
}

impl Clone for TemplateRendererAdapter {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
        }
    }
}
