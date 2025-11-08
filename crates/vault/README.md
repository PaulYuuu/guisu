# guisu-vault

Vault provider integrations for password managers.

## Architecture

This crate provides a unified interface (`SecretProvider` trait) for integrating various password managers into guisu. Each password manager is implemented as a separate module with feature flags for optional compilation.

## Supported Providers

| Provider | Feature | Status | Type | Notes |
|----------|---------|--------|------|-------|
| Bitwarden (bw) | `bitwarden-cli` | ✅ Stable | CLI-based | Full feature support |
| Rbw (Rust Bitwarden) | `rbw-cli` | ✅ Stable | CLI-based | See limitations below |
| Bitwarden Secrets Manager | `bws-cli` | ✅ Stable | CLI-based | For machine secrets |
| 1Password | `onepassword` | 🚧 Planned | CLI-based | - |
| LastPass | `lastpass` | 🚧 Planned | CLI-based | - |
| Bitwarden SDK | `bitwarden-sdk` | 🚧 Future | Native Rust | - |

## Adding a New Provider

### 1. Create a new module

Create `src/yourprovider.rs`:

```rust
use crate::{Error, Result, SecretProvider};
use serde_json::Value as JsonValue;
use std::process::Command;

pub struct YourProvider;

impl YourProvider {
    pub fn new() -> Self {
        Self
    }
}

impl SecretProvider for YourProvider {
    fn name(&self) -> &str {
        "yourprovider"
    }

    fn execute(&self, args: &[&str]) -> Result<JsonValue> {
        // Execute CLI and return JSON
        let output = Command::new("your-cli")
            .args(args)
            .output()?;

        if !output.status.success() {
            return Err(Error::ExecutionFailed(
                String::from_utf8_lossy(&output.stderr).into()
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(&stdout)
            .map_err(|e| Error::ParseError(e.to_string()))
    }

    fn is_available(&self) -> bool {
        Command::new("your-cli")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn help(&self) -> &str {
        "Your Provider CLI\n\
         \n\
         Requirements:\n\
         - Install: your installation instructions\n\
         \n\
         Usage in templates:\n\
         {{ yourprovider(\"get\", \"secret-name\") }}"
    }
}
```

### 2. Add feature flag

In `Cargo.toml`:

```toml
[features]
yourprovider = []
```

### 3. Add module to lib.rs

In `src/lib.rs`:

```rust
#[cfg(feature = "yourprovider")]
pub mod yourprovider;
```

### 4. Register in template engine

In `guisu-template/src/functions.rs`:

```rust
#[cfg(feature = "yourprovider")]
use vault::yourprovider::YourProvider;

static YOUR_CACHE: Mutex<Option<CachedSecretProvider<YourProvider>>> = Mutex::new(None);

pub fn yourprovider(args: &[Value]) -> Result<Value, minijinja::Error> {
    let cmd_args: Vec<&str> = args
        .iter()
        .filter_map(|v| v.as_str())
        .collect();

    let mut cache = YOUR_CACHE.lock().unwrap();
    if cache.is_none() {
        *cache = Some(CachedSecretProvider::new(YourProvider::new()));
    }

    let provider = cache.as_mut().unwrap();
    let result = provider.execute_cached(&cmd_args).map_err(convert_error)?;

    Ok(Value::from_serialize(&result))
}
```

In `guisu-template/src/engine.rs`:

```rust
#[cfg(feature = "yourprovider")]
env.add_function("yourprovider", functions::yourprovider);
```

### 5. Enable feature in template crate

In `guisu-template/Cargo.toml`:

```toml
[features]
default = ["bitwarden-cli", "bws-cli", "yourprovider"]
yourprovider = ["vault/yourprovider"]
```

## Known Limitations

### Rbw (Rust Bitwarden) Limitations

While rbw is a faster alternative to the official bw CLI, it has some limitations:

#### Warning: SSH Private Keys Not Supported

**Issue:** rbw does not return SSH private keys - only public keys and fingerprints.

**Rbw output:**
```json
{
  "type": 5,
  "data": {
    "public_key": "ssh-ed25519 AAAAC3...",
    "fingerprint": "SHA256:..."
  }
}
```

**Bw output:**
```json
{
  "type": 5,
  "sshKey": {
    "privateKey": "-----BEGIN PRIVATE KEY-----\n...",
    "publicKey": "ssh-ed25519 AAAAC3..."
  }
}
```

**In templates:**
```jinja2
# ❌ This works with bw but NOT with rbw:
{{ bitwarden("MySSH").sshKey.privateKey }}  # undefined with rbw

# ✅ This works with both bw and rbw:
{{ bitwarden("MySSH").sshKey.publicKey }}   # works with both
```

**Workaround:** Use the official bw CLI if you need SSH private keys in templates.

**Configuration:**
```toml
[bitwarden]
provider = "bw"  # Use bw instead of rbw for SSH private key support
```

#### Warning: Attachments Not Supported

**Issue:** rbw does not support downloading file attachments.

**In templates:**
```jinja2
# ❌ This only works with bw:
{{ bitwardenAttachment("config.json", "item-uuid") }}  # not supported in rbw
```

**Workaround:** Use the official bw CLI if you need file attachments.

## Design Principles

1. **Separation of Concerns**: Each password manager is isolated in its own module
2. **Feature Flags**: Users only compile what they need
3. **Unified Interface**: All providers implement the same `SecretProvider` trait
4. **Internal Format Transformation**: Each provider transforms its native output to a standardized format within its `execute()` method
5. **Caching**: Results are cached to avoid repeated CLI calls
6. **Error Handling**: Structured error types with helpful messages
7. **Future-Ready**: Architecture supports both CLI and native SDK providers

## Examples

### Using Bitwarden

```jinja
# Get entire item and access fields
GITHUB_USER={{ bitwarden("GitHub").login.username }}
GITHUB_TOKEN={{ bitwarden("GitHub").login.password }}

# Get specific custom field
API_KEY={{ bitwardenFields("GitHub", "APIKey") }}

# Get SSH keys
SSH_KEY={{ bitwarden("MySSH").sshKey.privateKey }}
```

### Using Bitwarden Secrets Manager

```jinja
# Get organization secret
API_SECRET={{ bitwardenSecrets("secret-uuid").value }}
```

### Future: Using 1Password

```jinja
# Template (planned)
DB_PASSWORD={{ onepassword("get", "database", "password") }}
```

## License

MIT OR Apache-2.0
