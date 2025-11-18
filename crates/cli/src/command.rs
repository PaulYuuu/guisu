//! Command trait for guisu CLI
//!
//! This module defines the `Command` trait that all guisu commands implement.
//! It provides a uniform interface for command execution, making it easier to
//! test, extend, and maintain commands.

use anyhow::Result;

use crate::common::RuntimeContext;

/// Trait for all guisu commands
///
/// All commands that require configuration and resolved paths should implement
/// this trait. The `execute` method receives a `RuntimeContext` containing
/// shared state like configuration and resolved paths.
///
/// # Example
///
/// ```rust,ignore
/// use crate::command::Command;
/// use crate::common::RuntimeContext;
/// use anyhow::Result;
/// use clap::Args;
///
/// #[derive(Debug, Args)]
/// pub struct MyCommand {
///     #[arg(short, long)]
///     pub some_flag: bool,
/// }
///
/// impl Command for MyCommand {
///     fn execute(&self, context: &RuntimeContext) -> Result<()> {
///         // Access config: context.config
///         // Access paths: context.source_dir(), context.dest_dir(), context.dotfiles_dir()
///         Ok(())
///     }
/// }
/// ```
pub trait Command {
    /// Execute the command with the given runtime context
    ///
    /// # Arguments
    ///
    /// * `context` - Runtime context containing configuration and resolved paths
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or an error describing what went wrong.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute. Error messages should
    /// be descriptive enough for the user to understand what went wrong.
    fn execute(&self, context: &RuntimeContext) -> Result<()>;
}
