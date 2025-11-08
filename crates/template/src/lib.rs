//! # Guisu Template
//!
//! Template engine integration for guisu using minijinja.
//!
//! This crate provides template rendering capabilities with custom functions
//! for accessing system information, environment variables, and more.

pub mod context;
pub mod engine;
pub mod functions;

pub use context::TemplateContext;
pub use engine::TemplateEngine;
pub use functions::set_source_dir;
pub use guisu_config::{AgeConfigInfo, BitwardenConfigInfo, ConfigInfo, UiConfigInfo};

use thiserror::Error;

/// Result type for template operations
pub type Result<T> = std::result::Result<T, Error>;

/// Template engine errors
#[derive(Error, Debug)]
pub enum Error {
    /// Template rendering error
    #[error("Template error at {location}: {message}")]
    Render { location: String, message: String },

    /// Template syntax error
    #[error("Template syntax error: {0}")]
    Syntax(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Other error
    #[error("{0}")]
    Other(String),
}

impl From<minijinja::Error> for Error {
    fn from(err: minijinja::Error) -> Self {
        // Extract detailed location information
        let location = match (err.name(), err.line(), err.range()) {
            (Some(name), Some(line), Some(range)) => {
                // Large column numbers indicate function return values being inlined.
                // For example, bitwardenFields() returns JSON that gets expanded in place.
                // The column refers to the expanded content, not the source line.
                // In such cases, omit the misleading column number.
                if range.start > 200 {
                    format!("{} line {}", name, line)
                } else {
                    format!("{} line {}, column {}", name, line, range.start)
                }
            }
            (Some(name), Some(line), None) => {
                format!("{} line {}", name, line)
            }
            (None, Some(line), Some(range)) => {
                if range.start > 200 {
                    format!("line {}", line)
                } else {
                    format!("line {}, column {}", line, range.start)
                }
            }
            (None, Some(line), None) => {
                format!("line {}", line)
            }
            _ => "unknown location".to_string(),
        };

        Error::Render {
            location,
            message: err.to_string(),
        }
    }
}
