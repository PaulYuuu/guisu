//! Custom template functions
//!
//! This module provides custom functions and filters for use in templates.

use guisu_crypto::{Identity, decrypt_inline, encrypt_inline};
use indexmap::IndexMap;
use minijinja::Value;
use serde_json::Value as JsonValue;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

// Secret providers
use guisu_vault::SecretProvider;
#[cfg(feature = "bws")]
use guisu_vault::{CachedSecretProvider, bws::BwsCli};

// Global source directory for include functions
static SOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

// Bitwarden cache structure with separated provider and cache
struct BitwardenCache {
    provider: Box<dyn SecretProvider>,
    cache: Mutex<IndexMap<String, JsonValue>>,
}

impl BitwardenCache {
    fn new(provider_name: &str) -> Result<Self, guisu_vault::Error> {
        let provider = Self::create_provider(provider_name)?;
        Ok(Self {
            provider,
            cache: Mutex::new(IndexMap::new()),
        })
    }

    /// Create Bitwarden provider based on configuration
    ///
    /// This is application-layer logic that chooses which provider implementation
    /// to use based on user configuration.
    fn create_provider(provider_name: &str) -> Result<Box<dyn SecretProvider>, guisu_vault::Error> {
        match provider_name {
            #[cfg(feature = "bw")]
            "bw" => Ok(Box::new(guisu_vault::bw::BwCli::new())),

            #[cfg(feature = "bw")]
            "rbw" => Ok(Box::new(guisu_vault::bw::RbwCli::new())),

            _ => {
                // Build list of available providers based on enabled features
                let providers = [
                    #[cfg(feature = "bw")]
                    "bw",
                    #[cfg(feature = "bw")]
                    "rbw",
                ];

                Err(guisu_vault::Error::ProviderNotAvailable(format!(
                    "Unknown Bitwarden Vault provider: '{}'. Valid options: {}",
                    provider_name,
                    providers.join(", ")
                )))
            }
        }
    }

    fn get_or_fetch(&self, cmd_args: &[&str]) -> Result<JsonValue, guisu_vault::Error> {
        let cache_key = cmd_args.join("|");

        // Quick read-only check
        if let Ok(cache) = self.cache.lock()
            && let Some(cached) = cache.get(&cache_key)
        {
            return Ok(cached.clone());
        }

        // Fetch and cache
        let result = self.provider.execute(cmd_args)?;

        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(cache_key, result.clone());
        }

        Ok(result)
    }
}

// Bitwarden cache singleton
// Since provider is configured once in config, we only need one cache instance
// The cache is initialized on first use with the configured provider
use std::collections::HashMap;
static BITWARDEN_CACHE: OnceLock<Mutex<HashMap<String, Arc<BitwardenCache>>>> = OnceLock::new();

// Cache for Bitwarden Secrets Manager CLI calls
#[cfg(feature = "bws")]
static BWS_CACHE: Mutex<Option<CachedSecretProvider<BwsCli>>> = Mutex::new(None);

/// Set the source directory for include functions
///
/// This should be called once before rendering templates that use include/includeTemplate.
pub fn set_source_dir(dir: PathBuf) {
    let _ = SOURCE_DIR.set(dir);
}

/// Get the configured source directory
fn get_source_dir() -> Option<&'static PathBuf> {
    SOURCE_DIR.get()
}

/// Convert vault error to minijinja error
fn convert_error(e: guisu_vault::Error) -> minijinja::Error {
    use guisu_vault::Error;
    match e {
        Error::Cancelled => minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Operation cancelled by user",
        ),
        Error::AuthenticationRequired(msg) => minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Authentication required: {}", msg),
        ),
        Error::ProviderNotAvailable(msg) => minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Provider not available: {}", msg),
        ),
        _ => minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()),
    }
}

/// Get an environment variable
///
/// Usage: `{{ env("PATH") }}`
pub fn env(name: &str) -> std::borrow::Cow<'static, str> {
    env::var(name)
        .map(std::borrow::Cow::Owned)
        .unwrap_or(std::borrow::Cow::Borrowed(""))
}

/// Get the operating system name
///
/// Usage: `{{ os() }}`
pub fn os() -> &'static str {
    #[cfg(target_os = "linux")]
    return "linux";

    #[cfg(target_os = "macos")]
    return "macos";

    #[cfg(target_os = "windows")]
    return "windows";

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return env::consts::OS;
}

/// Get the system architecture
///
/// Usage: `{{ arch() }}`
pub fn arch() -> &'static str {
    env::consts::ARCH
}

/// Get the system hostname
///
/// Usage: `{{ hostname() }}`
pub fn hostname() -> std::borrow::Cow<'static, str> {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .map(std::borrow::Cow::Owned)
        .unwrap_or(std::borrow::Cow::Borrowed("unknown"))
}

/// Get the current username
///
/// Usage: `{{ username() }}`
pub fn username() -> std::borrow::Cow<'static, str> {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .map(std::borrow::Cow::Owned)
        .unwrap_or(std::borrow::Cow::Borrowed("unknown"))
}

/// Get the home directory
///
/// Usage: `{{ home_dir() }}`
pub fn home_dir() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/home/unknown".to_string())
}

/// Join path components
///
/// Usage: `{{ joinPath("/home", "user", ".config") }}`
pub fn join_path(args: &[Value]) -> String {
    let mut path = PathBuf::new();
    for arg in args {
        if let Some(s) = arg.as_str() {
            path.push(s);
        }
    }
    path.to_string_lossy().to_string()
}

/// Look up an executable in PATH
///
/// Usage: `{{ lookPath("git") }}`
///
/// # Security
///
/// Input is validated to prevent command injection:
/// - Only alphanumeric characters, dashes, and underscores are allowed
/// - Path traversal attempts (..) are rejected
/// - Absolute paths are rejected
pub fn look_path(name: &str) -> Result<String, minijinja::Error> {
    // Validate input: only alphanumeric, dash, underscore
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "Invalid executable name: '{}'. Only alphanumeric characters, dashes, and underscores allowed",
                name
            ),
        ));
    }

    // Path traversal prevention
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Path traversal detected in executable name",
        ));
    }

    Ok(which::which(name)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default())
}

/// Always wrap a string in double quotes with proper escaping
///
/// This filter always adds double quotes around the value, escaping any
/// internal double quotes and backslashes.
///
/// Usage: `{{ some_var | quote }}`
///
/// Examples:
/// - `hello` → `"hello"`
/// - `say "hi"` → `"say \"hi\""`
/// - `path\to\file` → `"path\\to\\file"`
pub fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Convert a value to JSON
///
/// Usage: `{{ some_data | toJson }}`
pub fn to_json(value: Value) -> Result<String, minijinja::Error> {
    let json_value: serde_json::Value = serde_json::from_str(&value.to_string())
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));

    serde_json::to_string(&json_value)
        .map_err(|e| minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))
}

/// Parse a JSON string
///
/// Usage: `{{ json_string | fromJson }}`
pub fn from_json(value: &str) -> Result<Value, minijinja::Error> {
    let json_value: serde_json::Value = serde_json::from_str(value).map_err(|e| {
        minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
    })?;

    Ok(Value::from_serialize(&json_value))
}

/// Trim whitespace from both ends of a string
///
/// Removes leading and trailing whitespace (spaces, tabs, newlines, etc.).
///
/// # Usage
///
/// ```jinja2
/// {{ "  hello  " | trim }}  {# Output: "hello" #}
/// {{ someVar | trim }}
/// {{ bitwarden("item", "id").field | trim }}
/// ```
pub fn trim(value: &str) -> String {
    value.trim().to_string()
}

/// Trim whitespace from the start (left) of a string
///
/// Removes only leading whitespace.
///
/// # Usage
///
/// ```jinja2
/// {{ "  hello  " | trimStart }}  {# Output: "hello  " #}
/// {{ someVar | trimStart }}
/// ```
pub fn trim_start(value: &str) -> String {
    value.trim_start().to_string()
}

/// Trim whitespace from the end (right) of a string
///
/// Removes only trailing whitespace.
///
/// # Usage
///
/// ```jinja2
/// {{ "  hello  " | trimEnd }}  {# Output: "  hello" #}
/// {{ someVar | trimEnd }}
/// ```
pub fn trim_end(value: &str) -> String {
    value.trim_end().to_string()
}

/// Test if a string matches a regular expression
///
/// Returns true if the pattern matches anywhere in the string.
///
/// # Usage
///
/// ```jinja2
/// {{ regexMatch("hello123", "\\d+") }}  {# Output: true #}
/// {{ regexMatch(email, "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$") }}
/// ```
///
/// # Security
///
/// To prevent ReDoS (Regular Expression Denial of Service) attacks:
/// - Pattern length is limited to 200 characters
/// - Regex size is limited to 10MB
/// - DFA size is limited to 2MB
pub fn regex_match(text: &str, pattern: &str) -> Result<bool, minijinja::Error> {
    // Limit regex pattern complexity
    if pattern.len() > 200 {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Regex pattern too long ({} chars, max 200)", pattern.len()),
        ));
    }

    // Build regex with size limits to prevent catastrophic backtracking
    let re = regex::RegexBuilder::new(pattern)
        .size_limit(10 * (1 << 20)) // 10MB
        .dfa_size_limit(2 * (1 << 20)) // 2MB
        .build()
        .map_err(|e| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Invalid regex pattern: {}", e),
            )
        })?;

    Ok(re.is_match(text))
}

/// Replace all matches of a regular expression with a replacement string
///
/// # Usage
///
/// ```jinja2
/// {{ regexReplaceAll("hello 123 world 456", "\\d+", "X") }}  {# Output: "hello X world X" #}
/// {{ regexReplaceAll(text, "[aeiou]", "*") }}  {# Replace all vowels #}
/// ```
///
/// # Security
///
/// To prevent ReDoS (Regular Expression Denial of Service) attacks:
/// - Pattern length is limited to 200 characters
/// - Regex size is limited to 10MB
/// - DFA size is limited to 2MB
pub fn regex_replace_all(
    text: &str,
    pattern: &str,
    replacement: &str,
) -> Result<String, minijinja::Error> {
    // Limit regex pattern complexity
    if pattern.len() > 200 {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Regex pattern too long ({} chars, max 200)", pattern.len()),
        ));
    }

    // Build regex with size limits to prevent catastrophic backtracking
    let re = regex::RegexBuilder::new(pattern)
        .size_limit(10 * (1 << 20)) // 10MB
        .dfa_size_limit(2 * (1 << 20)) // 2MB
        .build()
        .map_err(|e| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Invalid regex pattern: {}", e),
            )
        })?;

    Ok(re.replace_all(text, replacement).to_string())
}

/// Split a string by a delimiter
///
/// Returns a list of strings.
///
/// # Usage
///
/// ```jinja2
/// {{ split("a,b,c", ",") }}  {# Output: ["a", "b", "c"] #}
/// {{ split("one:two:three", ":") | join(" - ") }}
/// {% for item in split(path, "/") %}
///   {{ item }}
/// {% endfor %}
/// ```
pub fn split(text: &str, delimiter: &str) -> Vec<String> {
    text.split(delimiter).map(|s| s.to_string()).collect()
}

/// Join a list of strings with a delimiter
///
/// # Usage
///
/// ```jinja2
/// {{ join(["a", "b", "c"], ", ") }}  {# Output: "a, b, c" #}
/// {{ items | join(" - ") }}
/// ```
pub fn join(items: Vec<Value>, delimiter: &str) -> String {
    items
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(delimiter)
}

/// Convert a value to TOML format
///
/// # Usage
///
/// ```jinja2
/// {{ config | toToml }}
/// {{ {"name": "value"} | toToml }}
/// ```
pub fn to_toml(value: Value) -> Result<String, minijinja::Error> {
    // Convert minijinja Value to serde_json::Value first
    let json_value: serde_json::Value = serde_json::from_str(&value.to_string())
        .or_else(|_| {
            // If direct parsing fails, try serializing the value
            serde_json::to_value(&value).map_err(|e| e.to_string())
        })
        .map_err(|e| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Failed to convert value: {}", e),
            )
        })?;

    // Convert to TOML
    toml::to_string(&json_value).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to serialize to TOML: {}", e),
        )
    })
}

/// Parse a TOML string
///
/// # Usage
///
/// ```jinja2
/// {{ toml_string | fromToml }}
/// {% set config = fromToml(file_content) %}
/// {{ config.database.host }}
/// ```
pub fn from_toml(value: &str) -> Result<Value, minijinja::Error> {
    let toml_value: toml::Value = toml::from_str(value).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to parse TOML: {}", e),
        )
    })?;

    // Convert TOML value to JSON value for better compatibility
    let json_value: serde_json::Value = serde_json::to_value(&toml_value).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to convert TOML to JSON: {}", e),
        )
    })?;

    Ok(Value::from_serialize(&json_value))
}

/// Access Bitwarden vault items
///
/// Returns the entire Bitwarden item object for direct access to any field.
/// Use dot notation to access nested properties.
///
/// # Usage
///
/// ```jinja2
/// {# Access SSH keys #}
/// {{ bitwarden("YvanYo-ssh").sshKey.publicKey }}
/// {{ bitwarden("YvanYo-ssh").sshKey.privateKey }}
///
/// {# Access login credentials #}
/// username = {{ bitwarden("Google").login.username }}
/// password = {{ bitwarden("Google").login.password }}
///
/// {# Access notes #}
/// {{ bitwarden("MyItem").notes }}
/// ```
///
/// # Arguments
///
/// - `item_id`: The name or UUID of the item
/// - `provider_name`: The Bitwarden provider to use ("bw" or "rbw")
///
/// # Environment
///
/// Requires either `bw` or `rbw` CLI to be installed and authenticated.
#[cfg(any(feature = "bw", feature = "rbw"))]
pub fn bitwarden(args: &[Value], provider_name: &str) -> Result<Value, minijinja::Error> {
    if args.is_empty() {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "bitwarden requires at least 1 argument: item_id",
        ));
    }

    let item_id = args[0].as_str().ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Item ID must be a string",
        )
    })?;

    // Return the raw item directly
    bitwarden_get_raw("item", item_id, provider_name)
}

/// Internal function to get raw Bitwarden item
#[cfg(any(feature = "bw", feature = "rbw"))]
fn bitwarden_get_raw(
    item_type: &str,
    item_id: &str,
    provider_name: &str,
) -> Result<Value, minijinja::Error> {
    // Build command arguments based on provider
    let cmd_args: Vec<&str> = if provider_name == "rbw" {
        // rbw uses: rbw get --raw <name>
        vec!["get", "--raw", item_id]
    } else {
        // bw uses: bw get <type> <name>
        vec!["get", item_type, item_id]
    };

    // Get or initialize cache for this provider
    let caches = BITWARDEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut caches = caches.lock().unwrap_or_else(|poisoned| {
        // Recover from poisoned lock - cache may be incomplete but we can rebuild it
        poisoned.into_inner()
    });

    // Get or create cache for this provider
    if !caches.contains_key(provider_name) {
        let new_cache = BitwardenCache::new(provider_name).map_err(convert_error)?;
        caches.insert(provider_name.to_string(), Arc::new(new_cache));
    }

    let cache = Arc::clone(caches.get(provider_name).expect("Cache was just inserted"));
    drop(caches); // Release lock before executing command

    // Fetch from cache
    let result = cache.get_or_fetch(&cmd_args).map_err(convert_error)?;

    Ok(Value::from_serialize(&result))
}

/// Get an attachment from a Bitwarden item
///
/// Retrieves an attachment file from a Bitwarden item using the Bitwarden CLI.
///
/// # Usage
///
/// ```jinja2
/// {# Get an attachment by filename and item ID #}
/// {{ bitwardenAttachment("config.json", "item-uuid") }}
///
/// {# Common use case: SSH keys, certificates, config files #}
/// {{ bitwardenAttachment("id_rsa", "ssh-keys-item") }}
/// {{ bitwardenAttachment("server.crt", "certificates") }}
/// ```
///
/// # Arguments
///
/// - `filename`: The name of the attachment file
/// - `item_id`: The name or UUID of the item containing the attachment
/// - `provider_name`: The Bitwarden provider to use ("bw" or "rbw")
///
/// # Command executed
///
/// This function executes: `bw get attachment <filename> --itemid <itemid> --raw`
///
/// # Important
///
/// - Only works with `bw` CLI (Bitwarden official CLI)
/// - `rbw` does not support attachments, so this function will fail if rbw is configured
/// - The attachment content is returned as a string
/// - Binary attachments will be returned as-is (you may need to handle encoding)
///
/// # Environment
///
/// Requires `bw` CLI to be installed and authenticated. The vault must be unlocked.
#[cfg(feature = "bw")]
pub fn bitwarden_attachment(
    args: &[Value],
    provider_name: &str,
) -> Result<String, minijinja::Error> {
    if args.len() < 2 {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "bitwardenAttachment requires 2 arguments: filename, item_id",
        ));
    }

    let filename = args[0].as_str().ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Filename must be a string",
        )
    })?;

    let item_id = args[1].as_str().ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Item ID must be a string",
        )
    })?;

    // rbw doesn't support attachments
    if provider_name == "rbw" {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "bitwardenAttachment is not supported with rbw. Please use bw (Bitwarden CLI) instead.",
        ));
    }

    // Build command: bw get attachment <filename> --itemid <itemid> --raw
    let cmd_args = vec!["get", "attachment", filename, "--itemid", item_id, "--raw"];

    // Get or initialize cache for this provider
    let caches = BITWARDEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut caches = caches.lock().unwrap_or_else(|poisoned| {
        // Recover from poisoned lock - cache may be incomplete but we can rebuild it
        poisoned.into_inner()
    });

    // Get or create cache for this provider
    if !caches.contains_key(provider_name) {
        let new_cache = BitwardenCache::new(provider_name).map_err(convert_error)?;
        caches.insert(provider_name.to_string(), Arc::new(new_cache));
    }

    let cache = Arc::clone(caches.get(provider_name).expect("Cache was just inserted"));
    drop(caches); // Release lock before executing command

    // Fetch from cache
    let result = cache.get_or_fetch(&cmd_args).map_err(convert_error)?;

    // Extract string content from result
    let content = if let Some(s) = result.as_str() {
        s.to_string()
    } else {
        result.to_string()
    };

    Ok(content)
}

/// Get a specific field from a Bitwarden item's fields array
///
/// Retrieves a value from the custom fields array in a Bitwarden item.
/// Also supports shorthand access to common fields like username, password, and notes.
///
/// # Usage
///
/// ```jinja2
/// {# Get custom fields from fields array #}
/// api_key = {{ bitwardenFields("Google", "APIKey") }}
/// project = {{ bitwardenFields("Google", "VertexProject") }}
///
/// {# Shorthand for common fields #}
/// username = {{ bitwardenFields("Google", "username") }}
/// password = {{ bitwardenFields("Google", "password") }}
/// notes = {{ bitwardenFields("Google", "notes") }}
/// ```
///
/// # Arguments
///
/// - `item_id`: The name or UUID of the item
/// - `field_name`: The name of the field to extract from the fields array
/// - `provider_name`: The Bitwarden provider to use ("bw" or "rbw")
///
/// # Note
///
/// For accessing top-level item properties (like sshKey), use `bitwarden()` instead:
/// `{{ bitwarden("YvanYo-ssh").sshKey.publicKey }}`
///
/// # Environment
///
/// Requires either `bw` or `rbw` CLI to be installed and authenticated.
pub fn bitwarden_fields(args: &[Value], provider_name: &str) -> Result<Value, minijinja::Error> {
    if args.len() < 2 {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "bitwardenFields requires 2 arguments: item_id, field_name",
        ));
    }

    let item_id = args[0].as_str().ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Item ID must be a string",
        )
    })?;

    let field_name = args[1].as_str().ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Field name must be a string",
        )
    })?;

    // Get the raw item
    let item = bitwarden_get_raw("item", item_id, provider_name)?;

    // Extract the specific field
    get_single_field(&item, field_name)
}

/// Get a single field from a Bitwarden item
fn get_single_field(item: &Value, field_name: &str) -> Result<Value, minijinja::Error> {
    // Try to get the field from common locations
    // First check custom fields
    if let Ok(fields) = item.get_attr("fields") {
        // Try to iterate if it's an array-like value
        if let Ok(iter) = fields.try_iter() {
            for field in iter {
                // Get name and value separately to avoid lifetime issues
                let name_result = field.get_attr("name");
                let value_result = field.get_attr("value");

                if let (Ok(name_val), Ok(value)) = (name_result, value_result)
                    && let Some(name) = name_val.as_str()
                    && name == field_name
                {
                    return Ok(value);
                }
            }
        }
    }

    // Check common shorthand fields
    match field_name {
        "username" => {
            if let Ok(login) = item.get_attr("login")
                && let Ok(username) = login.get_attr("username")
            {
                return Ok(username);
            }
        }
        "password" => {
            if let Ok(login) = item.get_attr("login")
                && let Ok(password) = login.get_attr("password")
            {
                return Ok(password);
            }
        }
        "notes" => {
            if let Ok(notes) = item.get_attr("notes") {
                return Ok(notes);
            }
        }
        _ => {}
    }

    Err(minijinja::Error::new(
        minijinja::ErrorKind::InvalidOperation,
        format!("Field '{}' not found in Bitwarden item", field_name),
    ))
}

/// Access Bitwarden Secrets Manager secrets
///
/// This function retrieves secrets from Bitwarden Secrets Manager (organization secrets).
/// This is separate from Bitwarden Vault (personal/team passwords).
///
/// # Usage
///
/// ```jinja2
/// {# Get a secret by ID #}
/// api_key = {{ bitwardenSecrets("secret-uuid") }}
///
/// {# Get secret value directly #}
/// api_key = {{ bitwardenSecrets("secret-uuid").value }}
/// ```
///
/// # Arguments
///
/// - `secret_id`: The secret ID/UUID
///
/// # Requirements
///
/// - Install: `cargo install bws`
/// - Set `BWS_ACCESS_TOKEN` environment variable with your machine account token
///
/// # Environment Variables
///
/// - `BWS_ACCESS_TOKEN`: Required - Your Bitwarden Secrets Manager access token
#[cfg(feature = "bws")]
pub fn bitwarden_secrets(args: &[Value]) -> Result<Value, minijinja::Error> {
    if args.is_empty() {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "bitwardenSecrets requires a secret ID",
        ));
    }

    let secret_id = args[0].as_str().ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Secret ID must be a string",
        )
    })?;

    // Build command: bws get <secret-id>
    let cmd_args = vec!["get", secret_id];

    // Get or create the cached provider
    let mut cache = BWS_CACHE.lock().unwrap_or_else(|poisoned| {
        // Recover from poisoned lock - cache may be lost but we can recreate it
        poisoned.into_inner()
    });

    if cache.is_none() {
        *cache = Some(CachedSecretProvider::new(BwsCli::new()));
    }

    let provider = cache.as_mut().ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Failed to initialize BWS cache provider",
        )
    })?;

    let result = provider.execute_cached(&cmd_args).map_err(convert_error)?;

    Ok(Value::from_serialize(&result))
}

/// Decrypt an inline encrypted value in format: `age:base64(...)`
///
/// This filter decrypts values that were encrypted with the `encrypt_inline` function
/// from the crypto module. The encrypted value must be in the compact format starting
/// with "age:" prefix.
///
/// # Usage
///
/// ```jinja2
/// {# Decrypt an encrypted password stored in source file #}
/// DATABASE_PASSWORD={{ "age:YWdlLWVuY3J5cHRpb24..." | decrypt }}
///
/// {# Can be combined with other filters #}
/// API_KEY={{ env("ENCRYPTED_KEY") | decrypt | trim }}
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - No identities are available for decryption
/// - The encrypted value format is invalid
/// - Decryption fails (wrong key, corrupted data)
///
/// # Note
///
/// This filter requires that the TemplateEngine was created with `with_identities()`.
/// If no identities are available, decryption will fail.
pub fn decrypt(value: &str, identities: &Arc<Vec<Identity>>) -> Result<String, minijinja::Error> {
    decrypt_inline(value, identities).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Decryption failed: {}", e),
        )
    })
}

/// Encrypt a plaintext value to inline encrypted format: `age:base64(...)`
///
/// This filter encrypts plaintext values using the age encryption format.
/// The result is a compact single-line format suitable for embedding in config files.
///
/// # Usage
///
/// ```jinja2
/// {# Encrypt a literal value #}
/// DATABASE_PASSWORD={{ "my-secret-password" | encrypt }}
///
/// {# Encrypt an environment variable #}
/// API_KEY={{ env("API_KEY") | encrypt }}
///
/// {# Encrypt a Bitwarden value #}
/// JWT_SECRET={{ bitwardenFields("item", "MyApp", "jwt_secret") | encrypt }}
///
/// {# Can be combined with other filters #}
/// TOKEN={{ env("TOKEN") | trim | encrypt }}
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - No identities are available for encryption
/// - Encryption fails
///
/// # Note
///
/// This filter requires that the TemplateEngine was created with `with_identities()`.
/// If no identities are available, encryption will fail.
///
/// The encrypted value will be different each time (due to encryption nonce),
/// even for the same plaintext.
pub fn encrypt(value: &str, identities: &Arc<Vec<Identity>>) -> Result<String, minijinja::Error> {
    if identities.is_empty() {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "No identity available for encryption.\n\
            \n\
            To fix this:\n\
            1. Generate a new identity:  guisu age generate\n\
            2. Or configure an existing identity in ~/.config/guisu/config.toml:\n\
            \n\
            [age]\n\
            identity = \"~/.ssh/id_ed25519\"  # Use SSH key\n\
            # or\n\
            identity = \"~/.config/guisu/key.txt\"  # Use age key",
        ));
    }

    let recipient = identities[0].to_public();
    encrypt_inline(value, &[recipient]).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Encryption failed: {}", e),
        )
    })
}

/// Validate that a path is safe to use (no traversal, within source dir)
fn validate_include_path(
    path: &str,
    source_dir: &std::path::Path,
) -> Result<std::path::PathBuf, minijinja::Error> {
    use std::path::Component;

    let requested_path = std::path::Path::new(path);

    // Reject absolute paths
    if requested_path.is_absolute() {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Absolute paths not allowed in include(): {}", path),
        ));
    }

    // Check for path traversal components
    for component in requested_path.components() {
        match component {
            Component::ParentDir => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Path traversal (..) not allowed in include(): {}", path),
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Invalid path component in include(): {}", path),
                ));
            }
            _ => {}
        }
    }

    let file_path = source_dir.join(path);

    // Final safety check: ensure resolved path is still within source_dir
    let canonical_file = fs::canonicalize(&file_path).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to resolve path '{}': {}", path, e),
        )
    })?;

    let canonical_source = fs::canonicalize(source_dir).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to resolve source directory: {}", e),
        )
    })?;

    if !canonical_file.starts_with(&canonical_source) {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "Path '{}' escapes source directory. Canonical path: {}",
                path,
                canonical_file.display()
            ),
        ));
    }

    Ok(canonical_file)
}

/// Include the contents of a file
///
/// Reads and includes the raw contents of a file from the source directory.
/// The file path is relative to the source directory.
///
/// Usage: `{{ include(".zshrc-common") }}`
///
/// # Arguments
///
/// - `path`: Relative path to the file from the source directory
///
/// # Examples
///
/// ```jinja2
/// # Include a common shell configuration
/// {{ include(".zshrc-common") }}
///
/// # Include platform-specific config
/// {{ include(".config/nvim/init.lua") }}
/// ```
///
/// # Security
///
/// This function validates the path to prevent directory traversal attacks:
/// - Absolute paths are rejected
/// - Path traversal (..) is not allowed
/// - The final resolved path must be within the source directory
///
/// # Errors
///
/// Returns an error if:
/// - Source directory is not configured
/// - Path contains invalid components (absolute, .., etc.)
/// - Path escapes the source directory
/// - File does not exist
/// - File cannot be read
pub fn include(path: &str) -> Result<String, minijinja::Error> {
    let source_dir = get_source_dir().ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Source directory not configured for include() function",
        )
    })?;

    let canonical_file = validate_include_path(path, source_dir)?;

    fs::read_to_string(&canonical_file).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to read file '{}': {}", path, e),
        )
    })
}

/// Include and render a template file
///
/// Reads a file from the source directory and renders it as a template.
/// The file path is relative to the source directory.
/// The template has access to the same context as the parent template.
///
/// Usage: `{{ includeTemplate(".zshrc-common.j2") }}`
///
/// # Arguments
///
/// - `path`: Relative path to the template file from the source directory
///
/// # Examples
///
/// ```jinja2
/// # Include and render a common template
/// {{ includeTemplate(".zshrc-common.j2") }}
///
/// # Include platform-specific template with variables
/// {{ includeTemplate(".config/git/config.j2") }}
/// ```
///
/// # Security
///
/// This function validates the path to prevent directory traversal attacks:
/// - Absolute paths are rejected
/// - Path traversal (..) is not allowed
/// - The final resolved path must be within the source directory
///
/// # Note
///
/// This function reads the file but returns it as-is. The actual template
/// rendering happens in the parent template context. To include a file
/// without template rendering, use `include()` instead.
///
/// For full template rendering with a separate context, use minijinja's
/// built-in `{% include %}` statement instead.
///
/// # Errors
///
/// Returns an error if:
/// - Source directory is not configured
/// - Path contains invalid components (absolute, .., etc.)
/// - Path escapes the source directory
/// - File does not exist
/// - File cannot be read
pub fn include_template(path: &str) -> Result<String, minijinja::Error> {
    // For now, includeTemplate just reads the file content
    // The rendering will happen in the context where it's used
    // This matches chezmoi's behavior where includeTemplate returns
    // the template content to be rendered in the current context
    include(path)
}
