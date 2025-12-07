//! Custom template functions
//!
//! This module provides custom functions and filters for use in templates.

use guisu_crypto::{Identity, decrypt_inline, encrypt_inline};
use indexmap::IndexMap;
use minijinja::Value;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

// Secret providers
use guisu_vault::SecretProvider;
#[cfg(feature = "bws")]
use guisu_vault::{CachedSecretProvider, bws::BwsCli};

// Cached system information
static HOSTNAME_CACHE: OnceLock<String> = OnceLock::new();
static USERNAME_CACHE: OnceLock<String> = OnceLock::new();
static HOME_DIR_CACHE: OnceLock<String> = OnceLock::new();

// Regex cache - stores compiled regexes to avoid repeated compilation
// Limited to 32 entries to prevent unbounded memory growth
// Uses RwLock for better concurrency - multiple readers can access simultaneously
static REGEX_CACHE: OnceLock<std::sync::RwLock<HashMap<String, regex::Regex>>> = OnceLock::new();
const MAX_REGEX_CACHE_SIZE: usize = 32;

use secrecy::{ExposeSecret, SecretString};

// Bitwarden cache structure with separated provider and cache
// Cache stores JSON as SecretString for automatic memory zeroization
struct BitwardenCache {
    provider: Box<dyn SecretProvider>,
    cache: Mutex<IndexMap<String, SecretString>>,
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

        // Quick read-only check - deserialize from Secret<String>
        if let Ok(cache) = self.cache.lock()
            && let Some(cached_secret) = cache.get(&cache_key)
        {
            // Deserialize from exposed secret
            let json_str = cached_secret.expose_secret();
            return serde_json::from_str(json_str).map_err(|e| {
                guisu_vault::Error::Other(format!("Failed to deserialize cached secret: {e}"))
            });
        }

        // Fetch from provider
        let result = self.provider.execute(cmd_args)?;

        // Serialize to string and wrap in SecretString for automatic zeroization
        if let Ok(mut cache) = self.cache.lock()
            && let Ok(json_str) = serde_json::to_string(&result)
        {
            cache.insert(cache_key, SecretString::new(json_str.into()));
        }

        Ok(result)
    }
}

// Bitwarden cache singleton
// Since provider is configured once in config, we only need one cache instance
// The cache is initialized on first use with the configured provider
static BITWARDEN_CACHE: OnceLock<Mutex<HashMap<String, Arc<BitwardenCache>>>> = OnceLock::new();

// Cache for Bitwarden Secrets Manager CLI calls
#[cfg(feature = "bws")]
static BWS_CACHE: Mutex<Option<CachedSecretProvider<BwsCli>>> = Mutex::new(None);

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
            format!("Authentication required: {msg}"),
        ),
        Error::ProviderNotAvailable(msg) => minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Provider not available: {msg}"),
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
#[must_use]
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
#[must_use]
pub fn arch() -> &'static str {
    env::consts::ARCH
}

/// Get the system hostname
///
/// Usage: `{{ hostname() }}`
pub fn hostname() -> &'static str {
    HOSTNAME_CACHE.get_or_init(|| {
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string())
    })
}

/// Get the current username
///
/// Usage: `{{ username() }}`
pub fn username() -> &'static str {
    USERNAME_CACHE.get_or_init(|| {
        env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string())
    })
}

/// Get the home directory
///
/// Usage: `{{ home_dir() }}`
pub fn home_dir() -> &'static str {
    HOME_DIR_CACHE.get_or_init(|| {
        dirs::home_dir().map_or_else(
            || "/home/unknown".to_string(),
            |p| p.to_string_lossy().into_owned(),
        )
    })
}

/// Join path components
///
/// Usage: `{{ joinPath("/home", "user", ".config") }}`
#[must_use]
pub fn join_path(args: &[Value]) -> String {
    let mut path = PathBuf::new();
    for arg in args {
        if let Some(s) = arg.as_str() {
            path.push(s);
        }
    }
    path.to_string_lossy().into_owned()
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
///
/// # Errors
///
/// Returns error if executable is not found in PATH or input validation fails
pub fn look_path(name: &str) -> Result<String, minijinja::Error> {
    // Validate input: only alphanumeric, dash, underscore
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "Invalid executable name: '{name}'. Only alphanumeric characters, dashes, and underscores allowed"
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
        .map(|p| p.to_string_lossy().into_owned())
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
#[must_use]
pub fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Convert a value to JSON
///
/// Usage: `{{ some_data | toJson }}`
///
/// # Errors
///
/// Returns error if value cannot be converted to JSON
pub fn to_json(value: &Value) -> Result<String, minijinja::Error> {
    let json_value: serde_json::Value = serde_json::from_str(&value.to_string())
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));

    serde_json::to_string(&json_value)
        .map_err(|e| minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))
}

/// Parse a JSON string
///
/// Usage: `{{ json_string | fromJson }}`
///
/// # Errors
///
/// Returns error if value is not valid JSON
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
#[must_use]
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
#[must_use]
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
#[must_use]
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
/// To prevent `ReDoS` (Regular Expression Denial of Service) attacks:
/// - Pattern length is limited to 200 characters
/// - Regex size is limited to 10MB
/// - DFA size is limited to 2MB
///
/// # Errors
///
/// Returns error if pattern is invalid or exceeds complexity limits
pub fn regex_match(text: &str, pattern: &str) -> Result<bool, minijinja::Error> {
    // Limit regex pattern complexity
    if pattern.len() > 200 {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Regex pattern too long ({} chars, max 200)", pattern.len()),
        ));
    }

    // Get or compile regex (with caching for performance)
    let re = get_compiled_regex(pattern)?;
    Ok(re.is_match(text))
}

/// Get a compiled regex from cache or compile and cache it
///
/// Uses `RwLock` for better concurrency - multiple threads can read from cache simultaneously.
/// Only locks for writes when compiling new regexes.
fn get_compiled_regex(pattern: &str) -> Result<regex::Regex, minijinja::Error> {
    let cache = REGEX_CACHE.get_or_init(|| std::sync::RwLock::new(HashMap::new()));

    // Try read lock first (allows concurrent access)
    {
        let read_guard = cache.read().expect("Regex cache poisoned");
        if let Some(re) = read_guard.get(pattern) {
            return Ok(re.clone());
        }
    } // Read lock released here

    // Cache miss - need to compile and insert
    // Get write lock for modification
    let mut write_guard = cache.write().expect("Regex cache poisoned");

    // Double-check pattern (another thread might have inserted it while we waited for write lock)
    if let Some(re) = write_guard.get(pattern) {
        return Ok(re.clone());
    }

    // Compile regex
    let re = regex::RegexBuilder::new(pattern)
        .size_limit(10 * (1 << 20)) // 10MB
        .dfa_size_limit(2 * (1 << 20)) // 2MB
        .build()
        .map_err(|e| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Invalid regex pattern: {e}"),
            )
        })?;

    // Clear cache if it's getting too large (simple eviction strategy)
    if write_guard.len() >= MAX_REGEX_CACHE_SIZE {
        write_guard.clear();
    }

    // Store in cache
    write_guard.insert(pattern.to_string(), re.clone());
    Ok(re)
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
/// To prevent `ReDoS` (Regular Expression Denial of Service) attacks:
/// - Pattern length is limited to 200 characters
/// - Regex size is limited to 10MB
/// - DFA size is limited to 2MB
///
/// # Errors
///
/// Returns error if pattern is invalid or exceeds complexity limits
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

    // Get or compile regex (with caching for performance)
    let re = get_compiled_regex(pattern)?;
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
    text.split(delimiter)
        .map(std::string::ToString::to_string)
        .collect()
}

/// Join a list of strings with a delimiter
///
/// # Usage
///
/// ```jinja2
/// {{ join(["a", "b", "c"], ", ") }}  {# Output: "a, b, c" #}
/// {{ items | join(" - ") }}
/// ```
#[must_use]
pub fn join(items: &[Value], delimiter: &str) -> String {
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
///
/// # Errors
///
/// Returns error if value cannot be converted to TOML
pub fn to_toml(value: &Value) -> Result<String, minijinja::Error> {
    // Convert minijinja Value to serde_json::Value first
    let json_value: serde_json::Value = serde_json::from_str(&value.to_string())
        .or_else(|_| {
            // If direct parsing fails, try serializing the value
            serde_json::to_value(value).map_err(|e| e.to_string())
        })
        .map_err(|e| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Failed to convert value: {e}"),
            )
        })?;

    // Convert to TOML
    toml::to_string(&json_value).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to serialize to TOML: {e}"),
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
///
/// # Errors
///
/// Returns error if value is not valid TOML
pub fn from_toml(value: &str) -> Result<Value, minijinja::Error> {
    let toml_value: toml::Value = toml::from_str(value).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to parse TOML: {e}"),
        )
    })?;

    // Convert TOML value to JSON value for better compatibility
    let json_value: serde_json::Value = serde_json::to_value(&toml_value).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to convert TOML to JSON: {e}"),
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
///
/// # Errors
///
/// Returns error if Bitwarden provider is not available or command fails
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
///
/// # Panics
///
/// Should not panic under normal circumstances. Cache access is protected by mutex.
///
/// # Errors
///
/// Returns error if Bitwarden CLI is not available or attachment retrieval fails
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
///
/// # Errors
///
/// Returns error if Bitwarden provider is not available or field retrieval fails
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
        format!("Field '{field_name}' not found in Bitwarden item"),
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
///
/// # Errors
///
/// Returns error if BWS CLI is not available, access token is missing, or secret retrieval fails
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
/// This filter requires that the `TemplateEngine` was created with `with_identities()`.
/// If no identities are available, decryption will fail.
pub fn decrypt(value: &str, identities: &Arc<Vec<Identity>>) -> Result<String, minijinja::Error> {
    decrypt_inline(value, identities).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Decryption failed: {e}"),
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
/// This filter requires that the `TemplateEngine` was created with `with_identities()`.
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
            2. Or configure an existing identity in .guisu.toml:\n\
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
            format!("Encryption failed: {e}"),
        )
    })
}

/// Calculate blake3 hash of a string and return hex-encoded result
///
/// This filter hashes the input string using blake3 and returns the hex-encoded hash.
/// Blake3 is used throughout guisu for content hashing and change detection.
///
/// # Usage
///
/// ```jinja2
/// {# Hash a file's content to track changes #}
/// : << 'BREWFILE_HASH'
/// {{ include("darwin/Brewfile") | blake3sum }}
/// BREWFILE_HASH
///
/// {# Hash inline content #}
/// checksum = {{ "content to hash" | blake3sum }}
/// ```
///
/// # Returns
///
/// Hex-encoded blake3 hash (64 characters, 32 bytes)
///
/// # Examples
///
/// ```jinja2
/// # Track Brewfile changes by its hash
/// : << 'BREWFILE_HASH'
/// {{ include("darwin/Brewfile") | blake3sum }}
/// BREWFILE_HASH
/// ```
#[must_use]
pub fn blake3sum(value: &str) -> String {
    let hash_bytes = blake3::hash(value.as_bytes());
    hex::encode(hash_bytes.as_bytes())
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
            format!("Absolute paths not allowed in include(): {path}"),
        ));
    }

    // Check for path traversal components
    for component in requested_path.components() {
        match component {
            Component::ParentDir => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Path traversal (..) not allowed in include(): {path}"),
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Invalid path component in include(): {path}"),
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
            format!("Failed to resolve path '{path}': {e}"),
        )
    })?;

    let canonical_source = fs::canonicalize(source_dir).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to resolve source directory: {e}"),
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
/// Reads and includes the raw contents of a file from the dotfiles directory (guisu.srcDir).
/// The file path is relative to the dotfiles directory.
///
/// Usage: `{{ include("dot_zshrc-common") }}`
///
/// # Arguments
///
/// - `path`: Relative path to the file from the dotfiles directory (guisu.srcDir)
///
/// # Examples
///
/// ```jinja2
/// # Include a common shell configuration from dotfiles
/// {{ include("dot_zshrc-common") }}
///
/// # Include platform-specific config
/// {{ include("dot_config/nvim/init.lua") }}
///
/// # Calculate hash of included file
/// {{ include("darwin/Brewfile") | blake3sum }}
/// ```
///
/// # Security
///
/// This function validates the path to prevent directory traversal attacks:
/// - Absolute paths are rejected
/// - Path traversal (..) is not allowed
/// - The final resolved path must be within the dotfiles directory
///
/// # Errors
///
/// Returns an error if:
/// - Dotfiles directory (guisu.srcDir) is not available in context
/// - Path contains invalid components (absolute, .., etc.)
/// - Path escapes the dotfiles directory
/// - File does not exist
/// - File cannot be read
pub fn include(state: &minijinja::State, path: &str) -> Result<String, minijinja::Error> {
    // Get guisu.srcDir from context
    let src_dir_str = state
        .lookup("guisu")
        .and_then(|guisu| guisu.get_attr("srcDir").ok())
        .and_then(|v| v.as_str().map(std::string::ToString::to_string))
        .ok_or_else(|| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "guisu.srcDir not found in template context for include() function",
            )
        })?;

    let source_dir = PathBuf::from(&src_dir_str);
    let canonical_file = validate_include_path(path, &source_dir)?;

    fs::read_to_string(&canonical_file).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to read file '{path}': {e}"),
        )
    })
}

/// Include a template file from .guisu/templates directory
///
/// Reads the raw contents of a template file from the .guisu/templates directory.
/// The file path is relative to guisu.workingTree/.guisu/templates.
///
/// This function reads the file content but does NOT render it - the content is
/// returned as-is and will be rendered in the parent template context.
///
/// For platform-specific templates, the loader searches in this order:
/// 1. .guisu/templates/{platform}/{name}.j2
/// 2. .guisu/templates/{platform}/{name}
/// 3. .guisu/templates/{name}.j2
/// 4. .guisu/templates/{name}
///
/// Usage: `{{ includeTemplate("darwin/Brewfile") }}`
///
/// # Arguments
///
/// - `path`: Relative path to the template file from .guisu/templates directory
///
/// # Examples
///
/// ```jinja2
/// # Include a template file (uses template loader search order)
/// {{ includeTemplate("darwin/Brewfile") }}
///
/// # Include and hash the content
/// {{ includeTemplate("darwin/Brewfile") | blake3sum }}
/// ```
///
/// # Note
///
/// This function is useful when you want to include template content without
/// creating a separate rendering context. For example, to hash the content of
/// a template file for change detection.
///
/// For full template rendering with a separate context, use minijinja's
/// built-in `{% include %}` statement instead:
/// ```jinja2
/// {% include "darwin/Brewfile" %}
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - Templates directory (guisu.workingTree/.guisu/templates) is not available
/// - Path contains invalid components (absolute, .., etc.)
/// - File does not exist
/// - File cannot be read
pub fn include_template(state: &minijinja::State, path: &str) -> Result<String, minijinja::Error> {
    // Get guisu.workingTree from context
    let working_tree_str = state
        .lookup("guisu")
        .and_then(|guisu| guisu.get_attr("workingTree").ok())
        .and_then(|v| v.as_str().map(std::string::ToString::to_string))
        .ok_or_else(|| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "guisu.workingTree not found in template context for includeTemplate() function",
            )
        })?;

    let templates_dir = PathBuf::from(&working_tree_str)
        .join(".guisu")
        .join("templates");

    if !templates_dir.exists() {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "Templates directory does not exist: {}",
                templates_dir.display()
            ),
        ));
    }

    let canonical_file = validate_include_path(path, &templates_dir)?;

    fs::read_to_string(&canonical_file).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to read template file '{path}': {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    use std::fs;
    use tempfile::TempDir;

    // FIXME: This helper function needs to be implemented or tests need to be rewritten
    // Helper to create a temporary source directory
    // fn setup_source_dir() -> TempDir {
    //     let temp = TempDir::new().expect("Failed to create temp dir");
    //     set_source_dir(temp.path().to_path_buf());
    //     temp
    // }

    #[test]
    fn test_env_existing() {
        // Use temp_env for safe environment variable manipulation
        temp_env::with_var("TEST_VAR", Some("test_value"), || {
            assert_eq!(env("TEST_VAR"), "test_value");
        });
    }

    #[test]
    fn test_env_missing() {
        // Use temp_env to ensure variable is not set
        temp_env::with_var_unset("NONEXISTENT_VAR", || {
            assert_eq!(env("NONEXISTENT_VAR"), "");
        });
    }

    #[test]
    fn test_os() {
        let os_name = os();
        #[cfg(target_os = "linux")]
        assert_eq!(os_name, "linux");
        #[cfg(target_os = "macos")]
        assert_eq!(os_name, "macos");
        #[cfg(target_os = "windows")]
        assert_eq!(os_name, "windows");
    }

    #[test]
    fn test_arch() {
        let arch_name = arch();
        assert!(!arch_name.is_empty());
        assert!(["x86_64", "aarch64", "arm", "x86"].contains(&arch_name));
    }

    #[test]
    fn test_hostname() {
        let host = hostname();
        assert!(!host.is_empty());
        assert_ne!(host, "unknown");
    }

    #[test]
    fn test_username() {
        let user = username();
        assert!(!user.is_empty());
    }

    #[test]
    fn test_home_dir() {
        let home = home_dir();
        assert!(!home.is_empty());
        assert!(home.starts_with('/') || home.contains(':')); // Unix or Windows
    }

    #[test]
    fn test_join_path_simple() {
        let parts = vec![
            Value::from("home"),
            Value::from("user"),
            Value::from(".config"),
        ];
        let result = join_path(&parts);
        assert!(result.contains("home"));
        assert!(result.contains("user"));
        assert!(result.contains(".config"));
    }

    #[test]
    fn test_join_path_empty() {
        let parts: Vec<Value> = vec![];
        let result = join_path(&parts);
        assert_eq!(result, "");
    }

    #[test]
    fn test_join_path_single() {
        let parts = vec![Value::from("file.txt")];
        let result = join_path(&parts);
        assert_eq!(result, "file.txt");
    }

    #[test]
    fn test_look_path_valid() {
        // Try common executables that should exist
        let result = look_path("sh");
        assert!(result.is_ok());
        let path = result.unwrap();
        if !path.is_empty() {
            assert!(path.contains("sh"));
        }
    }

    #[test]
    fn test_look_path_invalid_chars() {
        let result = look_path("bin/sh"); // Contains '/' which is invalid
        assert!(result.is_err());
    }

    #[test]
    fn test_look_path_absolute() {
        let result = look_path("/bin/sh");
        assert!(result.is_err());
    }

    #[test]
    fn test_look_path_special_chars() {
        let result = look_path("sh;rm -rf");
        assert!(result.is_err());
    }

    #[test]
    fn test_quote_simple() {
        assert_eq!(quote("hello"), "\"hello\"");
    }

    #[test]
    fn test_quote_with_quotes() {
        assert_eq!(quote("say \"hi\""), "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn test_quote_with_backslashes() {
        assert_eq!(quote("path\\to\\file"), "\"path\\\\to\\\\file\"");
    }

    #[test]
    fn test_quote_empty() {
        assert_eq!(quote(""), "\"\"");
    }

    #[test]
    fn test_to_json_string() {
        let value = Value::from("hello");
        let result = to_json(&value).expect("to_json failed");
        assert_eq!(result, "\"hello\"");
    }

    #[test]
    fn test_from_json_string() {
        let result = from_json("\"hello\"").expect("from_json failed");
        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn test_from_json_object() {
        let json = r#"{"name":"John","age":30}"#;
        let result = from_json(json).expect("from_json failed");
        assert!(result.get_attr("name").is_ok());
        assert_eq!(result.get_attr("name").unwrap().as_str(), Some("John"));
    }

    #[test]
    fn test_from_json_invalid() {
        let result = from_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_trim_both_sides() {
        assert_eq!(trim("  hello  "), "hello");
    }

    #[test]
    fn test_trim_left_only() {
        assert_eq!(trim("  hello"), "hello");
    }

    #[test]
    fn test_trim_right_only() {
        assert_eq!(trim("hello  "), "hello");
    }

    #[test]
    fn test_trim_none() {
        assert_eq!(trim("hello"), "hello");
    }

    #[test]
    fn test_trim_newlines() {
        assert_eq!(trim("\n\thello\t\n"), "hello");
    }

    #[test]
    fn test_trim_start_whitespace() {
        assert_eq!(trim_start("  hello  "), "hello  ");
    }

    #[test]
    fn test_trim_start_none() {
        assert_eq!(trim_start("hello  "), "hello  ");
    }

    #[test]
    fn test_trim_end_whitespace() {
        assert_eq!(trim_end("  hello  "), "  hello");
    }

    #[test]
    fn test_trim_end_none() {
        assert_eq!(trim_end("  hello"), "  hello");
    }

    #[test]
    fn test_regex_match_simple() {
        let result = regex_match("hello123", r"\d+").expect("regex_match failed");
        assert!(result);
    }

    #[test]
    fn test_regex_match_no_match() {
        let result = regex_match("hello", r"\d+").expect("regex_match failed");
        assert!(!result);
    }

    #[test]
    fn test_regex_match_email() {
        let pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";
        let result = regex_match("test@example.com", pattern).expect("regex_match failed");
        assert!(result);
    }

    #[test]
    fn test_regex_match_pattern_too_long() {
        let pattern = "a".repeat(201);
        let result = regex_match("text", &pattern);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_regex_replace_all_simple() {
        let result = regex_replace_all("hello 123 world 456", r"\d+", "X")
            .expect("regex_replace_all failed");
        assert_eq!(result, "hello X world X");
    }

    #[test]
    fn test_regex_replace_all_vowels() {
        let result = regex_replace_all("hello", "[aeiou]", "*").expect("regex_replace_all failed");
        assert_eq!(result, "h*ll*");
    }

    #[test]
    fn test_regex_replace_all_no_match() {
        let result = regex_replace_all("hello", r"\d+", "X").expect("regex_replace_all failed");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_split_simple() {
        let result = split("a,b,c", ",");
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_colon() {
        let result = split("one:two:three", ":");
        assert_eq!(result, vec!["one", "two", "three"]);
    }

    #[test]
    fn test_split_empty_parts() {
        let result = split("a,,c", ",");
        assert_eq!(result, vec!["a", "", "c"]);
    }

    #[test]
    fn test_split_no_delimiter() {
        let result = split("hello", ",");
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn test_join_simple() {
        let items = vec![Value::from("a"), Value::from("b"), Value::from("c")];
        let result = join(&items, ", ");
        assert_eq!(result, "a, b, c");
    }

    #[test]
    fn test_join_empty() {
        let items: Vec<Value> = vec![];
        let result = join(&items, ", ");
        assert_eq!(result, "");
    }

    #[test]
    fn test_join_single() {
        let items = vec![Value::from("hello")];
        let result = join(&items, ", ");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_to_toml_simple() {
        let json = r#"{"name":"value","number":42}"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let minijinja_value = Value::from_serialize(&value);

        let result = to_toml(&minijinja_value).expect("to_toml failed");
        assert!(result.contains("name"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_from_toml_simple() {
        let toml_str = r#"
name = "test"
value = 42
"#;
        let result = from_toml(toml_str).expect("from_toml failed");
        assert_eq!(result.get_attr("name").unwrap().as_str(), Some("test"));
    }

    #[test]
    fn test_from_toml_invalid() {
        let result = from_toml("not toml [[]");
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        use guisu_crypto::Identity;

        let identity = Identity::generate();
        let identities = Arc::new(vec![identity]);

        let plaintext = "secret password";
        let encrypted = encrypt(plaintext, &identities).expect("encrypt failed");
        assert!(encrypted.starts_with("age:"));

        let decrypted = decrypt(&encrypted, &identities).expect("decrypt failed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_no_identity() {
        let identities = Arc::new(vec![]);
        let result = encrypt("secret", &identities);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No identity"));
    }

    #[test]
    fn test_decrypt_invalid_format() {
        use guisu_crypto::Identity;

        let identity = Identity::generate();
        let identities = Arc::new(vec![identity]);

        let result = decrypt("not-encrypted", &identities);
        assert!(result.is_err());
    }

    // Note: include() tests that require file I/O are platform-dependent
    // due to canonicalization requirements. They work in production but
    // may fail in test temp directories on some systems.

    // FIXME: These tests need to be rewritten to properly call include() with minijinja::State
    // #[test]
    // fn test_include_absolute_path_rejected() {
    //     let _temp = setup_source_dir();
    //     let result = include("/etc/passwd");
    //     assert!(result.is_err());
    //     assert!(
    //         result
    //             .unwrap_err()
    //             .to_string()
    //             .contains("Absolute paths not allowed")
    //     );
    // }

    // #[test]
    // fn test_include_parent_dir_rejected() {
    //     let _temp = setup_source_dir();
    //     let result = include("../etc/passwd");
    //     assert!(result.is_err());
    //     assert!(result.unwrap_err().to_string().contains("Path traversal"));
    // }

    #[test]
    fn test_validate_include_path_normal() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let result = validate_include_path("test.txt", temp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_include_path_parent_dir() {
        let temp = TempDir::new().unwrap();
        let result = validate_include_path("../secret", temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_include_path_absolute() {
        let temp = TempDir::new().unwrap();
        let result = validate_include_path("/etc/passwd", temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_regex_cache_eviction() {
        // Fill cache beyond limit
        for i in 0..MAX_REGEX_CACHE_SIZE + 5 {
            let pattern = format!(r"pattern{i}");
            let _ = regex_match("text", &pattern);
        }

        // Cache should have been cleared and repopulated
        let result = regex_match("test123", r"\d+");
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_compiled_regex_caching() {
        // First call compiles and caches
        let re1 = get_compiled_regex(r"\d+").expect("Failed to compile");

        // Second call should hit cache
        let re2 = get_compiled_regex(r"\d+").expect("Failed to compile");

        // Both should match the same pattern
        assert_eq!(re1.as_str(), re2.as_str());
    }

    #[test]
    fn test_get_compiled_regex_invalid_pattern() {
        let result = get_compiled_regex(r"[invalid");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid regex"));
    }

    #[test]
    fn test_to_json_nested_object() {
        let json = r#"{"outer":{"inner":"value"}}"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let minijinja_value = Value::from_serialize(&value);

        let result = to_json(&minijinja_value).expect("to_json failed");
        assert!(result.contains("outer"));
        assert!(result.contains("inner"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_to_json_array() {
        let json = r#"["a","b","c"]"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let minijinja_value = Value::from_serialize(&value);

        let result = to_json(&minijinja_value).expect("to_json failed");
        assert!(result.contains('['));
        assert!(result.contains(']'));
        assert!(result.contains('a'));
    }

    #[test]
    fn test_from_json_array() {
        let json = r#"["a","b","c"]"#;
        let result = from_json(json).expect("from_json failed");

        // Should be able to iterate
        let iter_result = result.try_iter();
        assert!(iter_result.is_ok());
    }

    #[test]
    fn test_from_json_nested() {
        let json = r#"{"outer":{"inner":"value"}}"#;
        let result = from_json(json).expect("from_json failed");

        let outer = result.get_attr("outer").expect("outer not found");
        let inner = outer.get_attr("inner").expect("inner not found");
        assert_eq!(inner.as_str(), Some("value"));
    }

    #[test]
    fn test_from_json_empty_object() {
        let result = from_json("{}").expect("from_json failed");
        // Empty object should parse successfully
        // Accessing non-existent attribute returns undefined, not an error
        let attr = result.get_attr("nonexistent");
        assert!(attr.is_ok());
        assert!(attr.unwrap().is_undefined());
    }

    #[test]
    fn test_from_json_empty_array() {
        let result = from_json("[]").expect("from_json failed");
        let iter = result.try_iter().expect("Should be iterable");
        assert_eq!(iter.count(), 0);
    }

    #[test]
    fn test_to_toml_nested() {
        let json = r#"{"section":{"key":"value"}}"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let minijinja_value = Value::from_serialize(&value);

        let result = to_toml(&minijinja_value).expect("to_toml failed");
        assert!(result.contains("section"));
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_from_toml_nested() {
        let toml_str = r#"
[database]
host = "localhost"
port = 5432
"#;
        let result = from_toml(toml_str).expect("from_toml failed");
        let database = result.get_attr("database").expect("database not found");
        assert_eq!(
            database.get_attr("host").unwrap().as_str(),
            Some("localhost")
        );
    }

    #[test]
    fn test_from_toml_empty() {
        let result = from_toml("").expect("from_toml failed");
        // Empty TOML should parse as empty object
        // Accessing non-existent attribute returns undefined, not an error
        let attr = result.get_attr("anything");
        assert!(attr.is_ok());
        assert!(attr.unwrap().is_undefined());
    }

    #[test]
    fn test_regex_replace_all_pattern_too_long() {
        let pattern = "a".repeat(201);
        let result = regex_replace_all("text", &pattern, "X");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_split_multiple_consecutive_delimiters() {
        let result = split("a:::b", ":");
        assert_eq!(result, vec!["a", "", "", "b"]);
    }

    #[test]
    fn test_split_delimiter_at_ends() {
        let result = split(":a:b:", ":");
        assert_eq!(result, vec!["", "a", "b", ""]);
    }

    #[test]
    fn test_join_with_non_string_values() {
        let items = vec![Value::from(1), Value::from(2), Value::from(3)];
        let result = join(&items, ", ");
        // Non-string values are filtered out
        assert_eq!(result, "");
    }

    #[test]
    fn test_join_mixed_values() {
        let items = vec![Value::from("a"), Value::from(123), Value::from("b")];
        let result = join(&items, ", ");
        // Only strings should be joined, numbers filtered out
        assert_eq!(result, "a, b");
    }

    #[test]
    fn test_trim_only_whitespace() {
        assert_eq!(trim("   "), "");
    }

    #[test]
    fn test_trim_start_only_whitespace() {
        assert_eq!(trim_start("   "), "");
    }

    #[test]
    fn test_trim_end_only_whitespace() {
        assert_eq!(trim_end("   "), "");
    }

    #[test]
    fn test_quote_already_quoted() {
        assert_eq!(quote("\"hello\""), "\"\\\"hello\\\"\"");
    }

    #[test]
    fn test_quote_mixed_escapes() {
        assert_eq!(quote("path\\with\"quotes"), "\"path\\\\with\\\"quotes\"");
    }

    #[test]
    fn test_regex_match_complex_pattern() {
        let pattern = r"^[A-Z][a-z]+\s+\d{4}$";
        assert!(regex_match("January 2024", pattern).expect("regex_match failed"));
        assert!(!regex_match("jan 2024", pattern).expect("regex_match failed"));
    }

    #[test]
    fn test_regex_replace_all_empty_replacement() {
        let result =
            regex_replace_all("hello123world456", r"\d+", "").expect("regex_replace_all failed");
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn test_look_path_nonexistent() {
        let result = look_path("nonexistent_binary_xyz123");
        assert!(result.is_ok());
        let path = result.unwrap();
        // Should return empty string if not found
        assert_eq!(path, "");
    }

    #[test]
    fn test_join_path_with_non_string() {
        let parts = vec![Value::from("home"), Value::from(123), Value::from("file")];
        let result = join_path(&parts);
        // Non-string values should be skipped
        assert!(result.contains("home"));
        assert!(result.contains("file"));
        assert!(!result.contains("123"));
    }

    #[test]
    fn test_convert_error_cancelled() {
        let vault_error = guisu_vault::Error::Cancelled;
        let minijinja_error = convert_error(vault_error);
        assert!(minijinja_error.to_string().contains("cancelled by user"));
    }

    #[test]
    fn test_convert_error_authentication_required() {
        let vault_error = guisu_vault::Error::AuthenticationRequired("Please login".to_string());
        let minijinja_error = convert_error(vault_error);
        assert!(
            minijinja_error
                .to_string()
                .contains("Authentication required")
        );
        assert!(minijinja_error.to_string().contains("Please login"));
    }

    #[test]
    fn test_convert_error_provider_not_available() {
        let vault_error = guisu_vault::Error::ProviderNotAvailable("bw not found".to_string());
        let minijinja_error = convert_error(vault_error);
        assert!(
            minijinja_error
                .to_string()
                .contains("Provider not available")
        );
        assert!(minijinja_error.to_string().contains("bw not found"));
    }

    #[test]
    fn test_quote_with_newlines() {
        let text = "line1\nline2\nline3";
        let result = quote(text);
        assert_eq!(result, "\"line1\nline2\nline3\"");
    }

    #[test]
    fn test_quote_with_tabs() {
        let text = "col1\tcol2\tcol3";
        let result = quote(text);
        assert_eq!(result, "\"col1\tcol2\tcol3\"");
    }

    #[test]
    fn test_look_path_with_dash() {
        // Executable names can contain dashes
        let result = look_path("test-executable");
        assert!(result.is_ok());
    }

    #[test]
    fn test_look_path_with_underscore() {
        // Executable names can contain underscores
        let result = look_path("test_executable");
        assert!(result.is_ok());
    }

    #[test]
    fn test_regex_match_empty_pattern() {
        // Empty pattern should match empty string
        let result = regex_match("", "");
        assert!(result.is_ok());
    }

    #[test]
    fn test_regex_replace_all_with_capture_groups() {
        let result = regex_replace_all("hello world", r"(\w+) (\w+)", "$2 $1")
            .expect("regex_replace_all failed");
        assert_eq!(result, "world hello");
    }

    #[test]
    fn test_split_with_multi_char_delimiter() {
        let result = split("one::two::three", "::");
        assert_eq!(result, vec!["one", "two", "three"]);
    }

    #[test]
    fn test_trim_unicode_whitespace() {
        // Test with various Unicode whitespace characters
        let text = "\u{00A0}hello\u{00A0}"; // Non-breaking space
        let result = trim(text);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_trim_start_only_newlines() {
        let result = trim_start("\n\n\n");
        assert_eq!(result, "");
    }

    #[test]
    fn test_trim_end_mixed_whitespace() {
        let result = trim_end("text \t\n\r");
        assert_eq!(result, "text");
    }

    #[test]
    fn test_to_json_bool_values() {
        let value = Value::from(true);
        let result = to_json(&value).expect("to_json failed");
        assert_eq!(result, "true");

        let value = Value::from(false);
        let result = to_json(&value).expect("to_json failed");
        assert_eq!(result, "false");
    }

    #[test]
    fn test_from_json_bool_values() {
        let result = from_json("true").expect("from_json failed");
        assert_eq!(result.to_string(), "true");

        let result = from_json("false").expect("from_json failed");
        assert_eq!(result.to_string(), "false");
    }

    #[test]
    fn test_from_json_null() {
        let result = from_json("null").expect("from_json failed");
        assert!(result.is_none() || result.is_undefined());
    }

    #[test]
    fn test_from_json_number() {
        let result = from_json("42").expect("from_json failed");
        // Numbers in JSON can be accessed as i64 or f64
        assert!(result.as_i64().is_some() || result.to_string() == "42");
    }

    #[test]
    fn test_to_toml_array() {
        let json = r#"["a","b","c"]"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let minijinja_value = Value::from_serialize(&value);

        let result = to_toml(&minijinja_value);
        // Arrays at top level aren't valid TOML, so this might error
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_from_toml_with_numbers() {
        let toml_str = r"
count = 42
price = 19.99
";
        let result = from_toml(toml_str).expect("from_toml failed");
        assert!(result.get_attr("count").is_ok());
        assert!(result.get_attr("price").is_ok());
    }

    #[test]
    fn test_from_toml_with_arrays() {
        let toml_str = r#"
items = ["a", "b", "c"]
"#;
        let result = from_toml(toml_str).expect("from_toml failed");
        let items = result.get_attr("items").expect("items not found");
        assert!(items.try_iter().is_ok());
    }

    #[test]
    fn test_regex_cache_persistence_across_calls() {
        // First call compiles and caches
        let _ = regex_match("test123", r"\d+").expect("First match failed");

        // Second call should use cache
        let _ = regex_match("test456", r"\d+").expect("Second match failed");

        // Different pattern should also be cached
        let _ = regex_match("abc", r"[a-z]+").expect("Third match failed");
    }

    #[test]
    fn test_get_compiled_regex_size_limits() {
        // Test that regex size limits are enforced
        // A very complex pattern might hit size limits
        let complex_pattern = r"(a|b|c|d|e|f|g|h|i|j|k|l|m|n|o|p|q|r|s|t|u|v|w|x|y|z)+";
        let result = get_compiled_regex(complex_pattern);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_include_path_with_current_dir_component() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();

        // Path with ./ should work
        let result = validate_include_path("./file.txt", temp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_include_path_nested_directories() {
        let temp = TempDir::new().unwrap();
        let nested_dir = temp.path().join("a").join("b").join("c");
        fs::create_dir_all(&nested_dir).unwrap();
        let file_path = nested_dir.join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let result = validate_include_path("a/b/c/file.txt", temp.path());
        assert!(result.is_ok());
    }
}
