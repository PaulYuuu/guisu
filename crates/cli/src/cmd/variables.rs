//! Variables command implementation
//!
//! Display all template variables available to guisu templates.

use anyhow::{Context, Result};
use clap::Args;
use guisu_template::TemplateContext;
use owo_colors::OwoColorize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

use guisu_config::Config;

use crate::command::Command;
use crate::common::RuntimeContext;

/// Variables command arguments
#[derive(Debug, Args)]
pub struct VariablesCommand {
    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Show only builtin (system) variables
    #[arg(long)]
    pub builtin: bool,

    /// Show only user-defined variables
    #[arg(long)]
    pub user: bool,
}

impl Command for VariablesCommand {
    type Output = ();
    fn execute(&self, context: &RuntimeContext) -> crate::error::Result<()> {
        // Determine filter based on flags
        let filter = match (self.builtin, self.user) {
            (true, true) => VariableFilter::All, // Both = show all
            (true, false) => VariableFilter::BuiltinOnly,
            (false, true) => VariableFilter::UserOnly,
            (false, false) => VariableFilter::All, // Default = show all
        };

        run_impl(context.source_dir(), &context.config, self.json, filter).map_err(Into::into)
    }
}

/// Which variables to display
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VariableFilter {
    All,
    BuiltinOnly,
    UserOnly,
}

/// Data structure for variable output
#[derive(Debug, Serialize)]
struct VariableData {
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<SystemVariables>,
    #[serde(skip_serializing_if = "Option::is_none")]
    guisu: Option<GuisuVariables>,
    #[serde(flatten)]
    variables: BTreeMap<String, serde_json::Value>,
}

/// System variables extracted from TemplateContext
#[derive(Debug, Serialize)]
struct SystemVariables {
    os: String,
    #[serde(rename = "osFamily")]
    os_family: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    distro: String,
    #[serde(rename = "distroId", skip_serializing_if = "String::is_empty")]
    distro_id: String,
    #[serde(rename = "distroVersion", skip_serializing_if = "String::is_empty")]
    distro_version: String,
    arch: String,
    hostname: String,
    username: String,
    uid: String,
    gid: String,
    group: String,
    #[serde(rename = "homeDir")]
    home_dir: String,
}

/// Guisu runtime variables
#[derive(Debug, Serialize)]
struct GuisuVariables {
    #[serde(rename = "srcDir")]
    src_dir: String,
    #[serde(rename = "workingTree")]
    working_tree: String,
    #[serde(rename = "dstDir")]
    dst_dir: String,
    #[serde(rename = "rootEntry", skip_serializing_if = "Option::is_none")]
    root_entry: Option<String>,
}

/// Run the variables command (implementation)
fn run_impl(source_dir: &Path, config: &Config, json: bool, filter: VariableFilter) -> Result<()> {
    // Create template context to get system variables
    let context = TemplateContext::new();

    // Collect system variables (only if needed)
    let system_vars = if matches!(filter, VariableFilter::All | VariableFilter::BuiltinOnly) {
        Some(SystemVariables {
            os: context.system.os.clone(),
            os_family: context.system.os_family.clone(),
            distro: context.system.distro.clone(),
            distro_id: context.system.distro_id.clone(),
            distro_version: context.system.distro_version.clone(),
            arch: context.system.arch.clone(),
            hostname: context.system.hostname.clone(),
            username: context.system.username.clone(),
            uid: context.system.uid.clone(),
            gid: context.system.gid.clone(),
            group: context.system.group.clone(),
            home_dir: context.system.home_dir,
        })
    } else {
        None
    };

    // Collect guisu runtime variables (only if needed)
    let guisu_vars = if matches!(filter, VariableFilter::All | VariableFilter::BuiltinOnly) {
        let dest_dir = dirs::home_dir()
            .map(|p| crate::path_to_string(&p))
            .unwrap_or_else(|| "/home/unknown".to_string());

        // Get the full dotfiles directory (including rootEntry if configured)
        let dotfiles_dir = config.dotfiles_dir(source_dir);

        // Get git working tree
        let working_tree = guisu_engine::git::find_working_tree(source_dir)
            .unwrap_or_else(|| source_dir.to_path_buf());

        Some(GuisuVariables {
            src_dir: crate::path_to_string(&dotfiles_dir),
            working_tree: crate::path_to_string(&working_tree),
            dst_dir: dest_dir,
            root_entry: Some(crate::path_to_string(&config.general.root_entry)),
        })
    } else {
        None
    };

    // Get user-defined variables from both .guisu/variables/ and config (only if needed)
    let user_variables = if matches!(filter, VariableFilter::All | VariableFilter::UserOnly) {
        // Load variables from .guisu/variables/ directory
        let guisu_dir = source_dir.join(".guisu");
        let platform_name = guisu_core::platform::CURRENT_PLATFORM.os;

        let mut all_vars = if guisu_dir.exists() {
            guisu_config::variables::load_variables(&guisu_dir, platform_name)
                .context("Failed to load variables from .guisu/variables/")?
        } else {
            indexmap::IndexMap::new()
        };

        // Merge with config variables (config overrides)
        all_vars.extend(config.variables.iter().map(|(k, v)| (k.clone(), v.clone())));

        all_vars.into_iter().collect()
    } else {
        BTreeMap::new()
    };

    // Combine into output data structure
    let data = VariableData {
        system: system_vars,
        guisu: guisu_vars,
        variables: user_variables,
    };

    // Output based on flag
    if json {
        output_json(&data)?;
    } else {
        output_pretty(&data);
    }

    Ok(())
}

/// Output in pretty/table format
fn output_pretty(data: &VariableData) {
    // Collect all key-value pairs first to calculate max width
    let mut all_vars: Vec<(String, serde_json::Value)> = Vec::new();

    // Collect system variables
    if let Some(system) = &data.system {
        all_vars.push((
            "system.os".to_string(),
            serde_json::Value::String(system.os.clone()),
        ));
        all_vars.push((
            "system.osFamily".to_string(),
            serde_json::Value::String(system.os_family.clone()),
        ));

        if !system.distro.is_empty() {
            all_vars.push((
                "system.distro".to_string(),
                serde_json::Value::String(system.distro.clone()),
            ));
        }
        if !system.distro_id.is_empty() {
            all_vars.push((
                "system.distroId".to_string(),
                serde_json::Value::String(system.distro_id.clone()),
            ));
        }
        if !system.distro_version.is_empty() {
            all_vars.push((
                "system.distroVersion".to_string(),
                serde_json::Value::String(system.distro_version.clone()),
            ));
        }

        all_vars.push((
            "system.arch".to_string(),
            serde_json::Value::String(system.arch.clone()),
        ));
        all_vars.push((
            "system.hostname".to_string(),
            serde_json::Value::String(system.hostname.clone()),
        ));
        all_vars.push((
            "system.username".to_string(),
            serde_json::Value::String(system.username.clone()),
        ));
        all_vars.push((
            "system.uid".to_string(),
            serde_json::Value::String(system.uid.clone()),
        ));
        all_vars.push((
            "system.gid".to_string(),
            serde_json::Value::String(system.gid.clone()),
        ));
        all_vars.push((
            "system.group".to_string(),
            serde_json::Value::String(system.group.clone()),
        ));
        all_vars.push((
            "system.homeDir".to_string(),
            serde_json::Value::String(system.home_dir.clone()),
        ));
    }

    // Collect guisu variables
    if let Some(guisu) = &data.guisu {
        all_vars.push((
            "guisu.srcDir".to_string(),
            serde_json::Value::String(guisu.src_dir.clone()),
        ));
        all_vars.push((
            "guisu.workingTree".to_string(),
            serde_json::Value::String(guisu.working_tree.clone()),
        ));
        all_vars.push((
            "guisu.dstDir".to_string(),
            serde_json::Value::String(guisu.dst_dir.clone()),
        ));
        if let Some(ref root_entry) = guisu.root_entry {
            all_vars.push((
                "guisu.rootEntry".to_string(),
                serde_json::Value::String(root_entry.clone()),
            ));
        }
    }

    // Collect user variables
    let flattened_user = flatten_json_map(&data.variables, "");
    all_vars.extend(flattened_user);

    // Calculate maximum key length
    let max_key_len = all_vars.iter().map(|(k, _)| k.len()).max().unwrap_or(20);

    // Group variables by prefix
    let system_vars: Vec<_> = all_vars
        .iter()
        .filter(|(k, _)| k.starts_with("system."))
        .collect();
    let guisu_vars: Vec<_> = all_vars
        .iter()
        .filter(|(k, _)| k.starts_with("guisu."))
        .collect();
    let user_vars: Vec<_> = all_vars
        .iter()
        .filter(|(k, _)| !k.starts_with("system.") && !k.starts_with("guisu."))
        .collect();

    // Display system variables
    if !system_vars.is_empty() {
        println!("\n{}", "System variables:".bright_cyan().bold());
        println!("{}", "─".repeat(60).dimmed());
        for (key, value) in system_vars {
            print_variable_aligned(key, value, max_key_len);
        }
    }

    // Display guisu variables
    if !guisu_vars.is_empty() {
        println!("\n{}", "Guisu variables:".bright_cyan().bold());
        println!("{}", "─".repeat(60).dimmed());
        for (key, value) in guisu_vars {
            print_variable_aligned(key, value, max_key_len);
        }
    }

    // Display user variables
    if !user_vars.is_empty() {
        println!("\n{}", "User variables:".bright_cyan().bold());
        println!("{}", "─".repeat(60).dimmed());
        for (key, value) in user_vars {
            print_variable_aligned(key, value, max_key_len);
        }
    }

    println!();
}

/// Flatten nested JSON objects into dot-notation keys
fn flatten_json_map(
    map: &BTreeMap<String, serde_json::Value>,
    prefix: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut result = BTreeMap::new();

    for (key, value) in map {
        let full_key = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", prefix, key)
        };

        match value {
            serde_json::Value::Object(obj) => {
                // Recursively flatten nested objects
                let nested_map: BTreeMap<String, serde_json::Value> =
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                let flattened = flatten_json_map(&nested_map, &full_key);
                result.extend(flattened);
            }
            _ => {
                result.insert(full_key, value.clone());
            }
        }
    }

    result
}

/// Print a single variable in pretty format with dynamic alignment
fn print_variable_aligned(key: &str, value: &serde_json::Value, width: usize) {
    let formatted_value = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "...".to_string())
        }
    };

    println!(
        "  {:<width$} {}",
        key.bright_yellow(),
        formatted_value.bright_white(),
        width = width
    );
}

/// Output in JSON format
fn output_json(data: &VariableData) -> Result<()> {
    let json =
        serde_json::to_string_pretty(data).context("Failed to serialize variables to JSON")?;
    println!("{}", json);
    Ok(())
}
