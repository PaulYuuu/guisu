//! Variable loading from .guisu/variables/ directory structure

use crate::Result;
use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use std::fs;
use std::path::Path;

/// Load variables from .guisu/variables/ directory
///
/// Loading order:
/// 1. Load all *.toml from variables/ (all platforms)
/// 2. Load all *.toml from variables/{platform}/ (platform-specific, overwrites same keys)
pub fn load_variables(guisu_dir: &Path, platform: &str) -> Result<IndexMap<String, JsonValue>> {
    use rayon::prelude::*;

    let mut variables = IndexMap::new();

    let variables_dir = guisu_dir.join("variables");
    if !variables_dir.exists() {
        return Ok(variables);
    }

    // 1. Load platform-agnostic variables (parallel file reading + parsing)
    if let Ok(entries) = fs::read_dir(&variables_dir) {
        let paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();

        let loaded: Vec<_> = paths
            .par_iter()
            .filter_map(|path| load_variable_file(path).ok().flatten())
            .collect();

        for (file_stem, vars) in loaded {
            let wrapped =
                IndexMap::from([(file_stem, JsonValue::Object(vars.into_iter().collect()))]);
            merge_variables(&mut variables, wrapped);
        }
    }

    // 2. Load platform-specific variables (parallel, overwrites)
    let platform_dir = variables_dir.join(platform);
    if platform_dir.exists()
        && let Ok(entries) = fs::read_dir(&platform_dir)
    {
        let paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();

        let loaded: Vec<_> = paths
            .par_iter()
            .filter_map(|path| load_variable_file(path).ok().flatten())
            .collect();

        for (file_stem, vars) in loaded {
            let wrapped =
                IndexMap::from([(file_stem, JsonValue::Object(vars.into_iter().collect()))]);
            merge_variables(&mut variables, wrapped);
        }
    }

    Ok(variables)
}

/// Load a single variable file (TOML only)
/// Returns the file stem (name without extension) and the loaded variables
fn load_variable_file(path: &Path) -> Result<Option<(String, IndexMap<String, JsonValue>)>> {
    // Only process .toml files
    if path.extension().and_then(|s| s.to_str()) != Some("toml") {
        return Ok(None);
    }

    // Get the filename without extension
    let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) => stem.to_string(),
        None => return Ok(None),
    };

    let content = fs::read_to_string(path).map_err(|e| {
        guisu_core::Error::Message(format!("Failed to read {}: {}", path.display(), e))
    })?;

    let value: toml::Value = toml::from_str(&content).map_err(|e| {
        guisu_core::Error::Message(format!(
            "Failed to parse TOML from {}: {}",
            path.display(),
            e
        ))
    })?;

    let json_value = serde_json::to_value(value).map_err(|e| {
        guisu_core::Error::Message(format!("Failed to convert TOML to JSON: {}", e))
    })?;

    if let JsonValue::Object(map) = json_value {
        Ok(Some((file_stem, map.into_iter().collect())))
    } else {
        Ok(None)
    }
}

/// Deep merge two variable maps (second overwrites first on conflicts)
fn merge_variables(base: &mut IndexMap<String, JsonValue>, overlay: IndexMap<String, JsonValue>) {
    for (key, value) in overlay {
        match (base.get_mut(&key), &value) {
            (Some(JsonValue::Object(base_obj)), JsonValue::Object(overlay_obj)) => {
                // Recursively merge objects
                let mut base_map: IndexMap<String, JsonValue> =
                    base_obj.clone().into_iter().collect();
                let overlay_map: IndexMap<String, JsonValue> =
                    overlay_obj.clone().into_iter().collect();
                merge_variables(&mut base_map, overlay_map);
                *base_obj = base_map.into_iter().collect();
            }
            _ => {
                // Overwrite with new value
                base.insert(key, value);
            }
        }
    }
}
