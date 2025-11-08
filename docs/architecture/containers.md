# Container Architecture / 容器架构

[English](#english) | [中文](#中文)

---

<a name="english"></a>

## English

This document details the container architecture of Guisu - the 7 crates that make up the system and their responsibilities.

### Table of Contents

- [Architecture Overview](#architecture-overview)
- [Dependency Layers](#dependency-layers)
- [Crate Details](#crate-details)
  - [Layer 0: Foundation](#layer-0-foundation)
  - [Layer 1: Base Services](#layer-1-base-services)
  - [Layer 2: Processing](#layer-2-processing)
  - [Layer 3: Engine](#layer-3-engine)
  - [Layer 4: Interface](#layer-4-interface)
- [Dependency Graph](#dependency-graph)
- [Cross-Cutting Concerns](#cross-cutting-concerns)

---

### Architecture Overview

Guisu is organized as a Cargo workspace with 7 crates, following a strict layered architecture:

```
crates/
├── core/        # Layer 0: Foundation types
├── crypto/      # Layer 1: Age encryption
├── vault/       # Layer 1: Password managers
├── template/    # Layer 2: Template rendering
├── config/      # Layer 2: Configuration
├── engine/      # Layer 3: State management
└── cli/         # Layer 4: User interface
```

**Key Principles:**

1. **Unidirectional Dependencies**: Higher layers depend on lower layers, never the reverse
2. **No Circular Dependencies**: Clean dependency graph
3. **Single Responsibility**: Each crate has one clear purpose
4. **Minimal Public API**: Only expose what's necessary

---

### Dependency Layers

```
┌─────────────────────────────────────┐
│  Layer 4: Interface                 │
│  ┌─────────────┐                    │
│  │  guisu-cli  │  Binary executable │
│  └──────┬──────┘                    │
└─────────┼────────────────────────────┘
          │
┌─────────┼────────────────────────────┐
│  Layer 3│ Engine                     │
│  ┌──────▼──────┐                     │
│  │guisu-engine │  State management  │
│  └──────┬──────┘                     │
└─────────┼────────────────────────────┘
          │
┌─────────┼────────────────────────────┐
│  Layer 2│ Processing                 │
│  ┌──────▼────────┐  ┌──────────────┐ │
│  │guisu-template │  │guisu-config  │ │
│  └───────────────┘  └──────────────┘ │
└─────────┬──────────────────────────┬─┘
          │                          │
┌─────────┼──────────────────────────┼─┐
│  Layer 1│ Base Services            │ │
│  ┌──────▼──────┐   ┌───────────────▼┐│
│  │guisu-crypto │   │  guisu-vault   ││
│  └──────┬──────┘   └───────────────┬┘│
└─────────┼──────────────────────────┼─┘
          │                          │
┌─────────┼──────────────────────────┼─┐
│  Layer 0│ Foundation               │ │
│  ┌──────▼──────────────────────────▼┐ │
│  │         guisu-core               │ │
│  │   (Shared by all crates)         │ │
│  └─────────────────────────────────┘ │
└─────────────────────────────────────┘
```

---

### Crate Details

#### Layer 0: Foundation

##### `guisu-core`

**Purpose**: Foundation types and utilities shared across all crates.

**Responsibilities**:
- Type-safe path types (`AbsPath`, `RelPath`, `SourceRelPath`)
- Platform detection (`Platform`, `CURRENT_PLATFORM`)
- Core error types
- Common utilities

**Public API**:

```rust
// Path types (NewType pattern)
pub struct AbsPath(PathBuf);      // Absolute paths
pub struct RelPath(PathBuf);      // Relative paths
pub struct SourceRelPath(PathBuf);// Source-relative paths

impl AbsPath {
    pub fn new(path: impl AsRef<Path>) -> Result<Self>;
    pub fn join(&self, rel: &RelPath) -> Self;
    pub fn strip_prefix(&self, base: &AbsPath) -> Result<RelPath>;
}

// Platform detection
pub struct Platform {
    pub os: &'static str,    // "darwin", "linux", "windows"
    pub arch: &'static str,  // "x86_64", "aarch64", "arm"
}

pub const CURRENT_PLATFORM: Platform;

// Error types
pub enum Error {
    PathNotAbsolute { path: PathBuf },
    PathNotRelative { path: PathBuf },
    InvalidPathPrefix { path: PathBuf, base: PathBuf },
    Message(String),
}
```

**Dependencies**: None (only std library)

**Lines of Code**: ~300

**Design Notes**:
- NewType pattern prevents mixing absolute/relative paths at compile time
- Platform detection is done once at compile time
- Zero runtime overhead for path types

---

#### Layer 1: Base Services

##### `guisu-crypto`

**Purpose**: Age encryption and decryption for files and inline values.

**Responsibilities**:
- Load age identities (SSH keys, age keys)
- Decrypt age-encrypted files
- Encrypt files for multiple recipients
- Inline encryption for config values

**Public API**:

```rust
// Identity management
pub fn load_identities(
    path: &Path,
    is_ssh: bool,
) -> Result<Vec<Box<dyn Identity>>>;

pub fn derive_recipients(
    identities: &[Box<dyn Identity>],
) -> Result<Vec<Box<dyn Recipient>>>;

// File encryption/decryption
pub fn decrypt_file_content(
    content: &[u8],
    identities: &[Box<dyn Identity>],
) -> Result<Vec<u8>>;

pub fn encrypt_file_content(
    content: &[u8],
    recipients: &[Box<dyn Recipient>],
) -> Result<Vec<u8>>;

// Inline encryption (for config values)
pub fn decrypt_inline(
    inline: &str,
    identities: &[Box<dyn Identity>],
) -> Result<String>;

pub fn encrypt_inline(
    value: &str,
    recipients: &[Box<dyn Recipient>],
) -> Result<String>;
```

**Dependencies**:
- `guisu-core`: Path types
- `age`: Encryption library (v0.11+)

**Lines of Code**: ~500

**Design Notes**:
- Supports both SSH keys and native age keys
- Multiple identities (try each until one works)
- Multiple recipients (encrypt once, decrypt with any key)
- Armor format for human-readable encrypted data

---

##### `guisu-vault`

**Purpose**: Password manager integrations for fetching secrets.

**Responsibilities**:
- Abstract interface for secret providers
- Bitwarden CLI integrations (bw, rbw, bws)
- Caching layer for expensive vault operations
- Command execution and JSON parsing

**Public API**:

```rust
// Trait for vault providers
pub trait SecretProvider: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn execute(&self, args: &[&str]) -> Result<serde_json::Value>;
    fn help(&self) -> &str;
}

// Implementations
#[cfg(feature = "bw")]
pub struct BwCli;      // Official Bitwarden CLI

#[cfg(feature = "rbw")]
pub struct RbwCli;     // Rust Bitwarden CLI

#[cfg(feature = "bws")]
pub struct BwsCli;     // Bitwarden Secrets Manager

// Caching wrapper
pub struct CachedSecretProvider<P: SecretProvider> {
    provider: P,
    cache: IndexMap<String, serde_json::Value>,
}

impl<P: SecretProvider> CachedSecretProvider<P> {
    pub fn new(provider: P) -> Self;
    pub fn execute(&mut self, args: &[&str]) -> Result<serde_json::Value>;
    pub fn clear_cache(&mut self);
}
```

**Dependencies**:
- Optional dependency on `guisu-core` (can be used standalone)

**Lines of Code**: ~400

**Design Notes**:
- Trait-based design allows easy addition of new providers
- Caching prevents redundant vault operations (expensive)
- Feature flags for optional providers (`bw`, `rbw`, `bws`)
- JSON-based API (vault CLIs return JSON)

---

#### Layer 2: Processing

##### `guisu-template`

**Purpose**: Template rendering using minijinja (Jinja2-compatible).

**Responsibilities**:
- Initialize minijinja environment
- Register template functions (~30 functions)
- Provide template context
- Platform-aware template loading
- Template rendering with error handling

**Public API**:

```rust
pub struct TemplateEngine {
    env: Environment<'static>,
}

impl TemplateEngine {
    // Constructors
    pub fn new() -> Self;

    pub fn with_identities_and_template_dir(
        identities: Arc<Vec<Box<dyn Identity>>>,
        template_dir: Option<PathBuf>,
    ) -> Self;

    pub fn with_identities_arc_template_dir_and_bitwarden_provider(
        identities: Arc<Vec<Box<dyn Identity>>>,
        template_dir: Option<PathBuf>,
        bitwarden_provider: &str,
    ) -> Self;

    // Rendering
    pub fn render_str(
        &self,
        template: &str,
        context: &TemplateContext,
    ) -> Result<String>;

    pub fn render_named_str(
        &self,
        name: &str,
        template: &str,
        context: &TemplateContext,
    ) -> Result<String>;
}

// Template context
#[derive(Debug, Clone, Serialize)]
pub struct TemplateContext {
    pub hostname: String,
    pub username: String,
    pub os: String,
    pub arch: String,
    pub source_dir: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    pub config: ConfigInfo,

    #[serde(flatten)]
    pub variables: IndexMap<String, Value>,
}
```

**Registered Functions** (~30 total):

| Category | Functions |
|----------|-----------|
| System | `os()`, `arch()`, `hostname()`, `username()`, `home_dir()` |
| Environment | `env(name)`, `lookPath(cmd)` |
| Paths | `joinPath(parts...)` |
| Bitwarden | `bitwarden(args)`, `bitwardenFields(args)`, `bitwardenAttachment()`, `bitwardenSecrets()` |
| Templates | `include(name)`, `includeTemplate(name)` |
| Encryption | `decrypt(value)`, `encrypt(value)` |
| String | `regexMatch()`, `regexReplaceAll()`, `split()`, `join()`, `quote`, `trim` |
| Data Formats | `toJson`, `fromJson`, `toToml`, `fromToml` |

**Dependencies**:
- `guisu-core`: Platform detection, path types
- `guisu-crypto`: Decrypt/encrypt functions
- `guisu-vault`: Bitwarden integration
- `minijinja`: Template engine (v2.12+)

**Lines of Code**: ~1,200

**Design Notes**:
- Platform-aware template loading (search `darwin/` then fallback)
- Jinja2-compatible syntax (familiar to most developers)
- Rich function library (30 functions vs 200+ in chezmoi - room to grow)
- Arc-based identity sharing (no cloning)

---

##### `guisu-config`

**Purpose**: Configuration loading, parsing, and management.

**Responsibilities**:
- Load `.guisu.toml` or `.guisu.toml.j2`
- Parse TOML configuration
- Load platform-specific variables
- Merge variables with smart section-based merging
- Resolve paths (relative → absolute)
- Provide configuration to other crates

**Public API**:

```rust
pub struct Config {
    pub general: GeneralConfig,
    pub age: AgeConfig,
    pub bitwarden: BitwardenConfig,
    pub ui: UiConfig,
    pub ignore: IgnoreConfig,
    pub variables: IndexMap<String, Value>,
}

impl Config {
    // Loading
    pub fn load_from_source(source_dir: &Path) -> Result<Self>;
    pub fn load_with_variables(source_dir: &Path) -> Result<Self>;

    // Utilities
    pub fn to_config_info(&self) -> ConfigInfo;
    pub fn platform_ignore_patterns(&self) -> (Vec<String>, Vec<String>);
    pub fn is_ignored(&self, path: &Path) -> bool;
}

// Sub-configurations
pub struct GeneralConfig {
    pub src_dir: Option<PathBuf>,
    pub dst_dir: Option<PathBuf>,
    pub root_entry: Option<PathBuf>,
    pub color: bool,
    pub progress: bool,
    pub use_builtin_age: AutoBool,
    pub use_builtin_git: AutoBool,
    pub editor: Option<String>,
    pub editor_args: Vec<String>,
}

pub struct AgeConfig {
    pub identity: Option<PathBuf>,
    pub identities: Option<Vec<PathBuf>>,
    pub recipient: Option<String>,
    pub recipients: Vec<String>,
    pub symmetric: bool,
    pub passphrase: bool,
}

pub struct BitwardenConfig {
    pub provider: String,  // "bw", "rbw", "bws"
}

pub struct UiConfig {
    pub pager: Option<String>,
    pub diff_tool: Option<String>,
}

pub struct IgnoreConfig {
    pub global: Vec<String>,
    pub darwin: Vec<String>,
    pub linux: Vec<String>,
    pub windows: Vec<String>,
}
```

**Variable Loading**:

```
.guisu/variables/
├── user.toml              # Global user variables
├── visual.toml            # Global visual settings
├── darwin/
│   ├── git.toml          # macOS git settings
│   └── terminal.toml     # macOS terminal settings
└── linux/
    ├── git.toml          # Linux git settings
    └── terminal.toml     # Linux terminal settings
```

**Merge Strategy**:
- Global variables loaded first
- Platform variables override by section name
- Different sections remain independent

**Dependencies**:
- `guisu-core`: Path types, platform detection
- `guisu-crypto`: Identity loading
- `guisu-template`: Template rendering (for `.guisu.toml.j2`)
- `serde`: Serialization framework
- `toml`: TOML parsing

**Lines of Code**: ~2,000

**Design Notes**:
- Support for templated config (`.guisu.toml.j2`)
- Platform-aware variable loading
- Smart merging (section-based, not wholesale override)
- Path resolution (relative → absolute, `~/` → home)

---

#### Layer 3: Engine

##### `guisu-engine`

**Purpose**: Core dotfile processing engine implementing the three-state model.

**Responsibilities**:
- Source state management (repository files)
- Target state management (desired state after processing)
- Destination state management (actual filesystem)
- Persistent state management (redb database)
- Content processing pipeline
- Entry type definitions
- File attribute parsing
- Three-way comparison logic

**Public API**:

```rust
// State types
pub struct SourceState {
    root: AbsPath,
    entries: HashMap<RelPath, SourceEntry>,
}

pub struct TargetState {
    entries: HashMap<RelPath, TargetEntry>,
}

pub struct DestinationState {
    root: AbsPath,
    cache: HashMap<RelPath, DestEntry>,
}

// Entry types
pub enum SourceEntry {
    File {
        source_path: SourceRelPath,
        target_path: RelPath,
        attributes: FileAttributes,
    },
    Directory {
        source_path: SourceRelPath,
        target_path: RelPath,
        attributes: FileAttributes,
    },
    Symlink {
        source_path: SourceRelPath,
        target_path: RelPath,
        link_target: PathBuf,
    },
}

pub enum TargetEntry {
    File {
        path: RelPath,
        content: Vec<u8>,
        mode: Option<u32>,
    },
    Directory {
        path: RelPath,
        mode: Option<u32>,
    },
    Symlink {
        path: RelPath,
        target: PathBuf,
    },
    Remove {
        path: RelPath,
    },
}

// File attributes (bitflags)
bitflags::bitflags! {
    pub struct FileAttributes: u8 {
        const DOT        = 1 << 0;  // Hidden file
        const PRIVATE    = 1 << 1;  // Mode 0600/0700
        const READONLY   = 1 << 2;  // Mode 0444
        const EXECUTABLE = 1 << 3;  // Mode 0755
        const TEMPLATE   = 1 << 4;  // .j2 extension
        const ENCRYPTED  = 1 << 5;  // .age extension
    }
}

// Content processing
pub struct ContentProcessor<D, R>
where
    D: Decryptor,
    R: TemplateRenderer,
{
    decryptor: D,
    renderer: R,
}

impl<D, R> ContentProcessor<D, R> {
    pub fn process_file(
        &self,
        source_path: &AbsPath,
        attrs: &FileAttributes,
        context: &serde_json::Value,
    ) -> Result<Vec<u8>>;
}

// Persistent state
pub trait PersistentState: Send + Sync {
    fn get(&self, bucket: &str, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn set(&self, bucket: &str, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, bucket: &str, key: &[u8]) -> Result<()>;
    fn delete_bucket(&self, bucket: &str) -> Result<()>;
    fn for_each<F>(&self, bucket: &str, f: F) -> Result<()>;
}

pub struct EntryState {
    pub content_hash: Vec<u8>,  // SHA256
    pub mode: Option<u32>,
}
```

**Processing Pipeline**:

```
1. SourceState::read(source_dir)
   - Walk directory (parallel with rayon)
   - Parse attributes from filename
   - Create SourceEntry objects

2. TargetState::from_source(source, processor, context)
   - Process files (parallel with rayon)
   - Decrypt .age files
   - Render .j2 templates
   - Create TargetEntry objects

3. DestinationState::read(dest_dir, target)
   - Read actual files from disk
   - Create DestEntry objects

4. Compare Target ↔ Destination ↔ Database
   - Three-way comparison
   - Detect: Synced, Modified, Added, Removed, Conflict

5. Apply changes
   - Write files
   - Set permissions
   - Update database
```

**Dependencies**:
- `guisu-core`: All path types, platform detection
- `guisu-crypto`: Age decryption
- `guisu-template`: Template rendering
- `guisu-config`: Configuration
- `guisu-vault`: Optional for template context
- `redb`: Persistent database
- `rayon`: Parallel processing
- `walkdir`: Directory traversal

**Lines of Code**: ~4,000

**Design Notes**:
- Trait-based content processing (pluggable decryptor/renderer)
- Parallel processing for I/O-bound operations
- Three-state model enables sophisticated change detection
- Persistent state tracks content hashes (not full content)
- Bitflags for efficient attribute storage

---

#### Layer 4: Interface

##### `guisu-cli`

**Purpose**: Command-line interface and user interaction.

**Responsibilities**:
- CLI argument parsing
- Command implementations
- Interactive TUI (conflict resolution)
- Progress reporting
- Error display
- Git operations
- Statistics tracking

**Commands Implemented**:

```
guisu
├── age/          # Encryption management
│   ├── generate  # Generate age identity
│   ├── encrypt   # Encrypt value
│   ├── decrypt   # Decrypt value
│   └── recipients# List recipients
├── add           # Add files to source
├── apply         # Apply changes
├── cat           # Display processed files
├── config/       # Configuration management
│   └── get       # Get config value
├── diff          # Show differences
├── edit          # Edit source files
├── hooks/        # Hook management (future)
├── ignored/      # Ignore patterns
├── info          # System information
├── init          # Initialize repository
├── status        # Show file status
├── templates/    # Template management
│   └── execute   # Execute template
├── update        # Pull and apply
└── variables     # Show template variables
```

**Public API** (library mode):

```rust
// Command modules
pub mod commands {
    pub mod add;
    pub mod apply;
    pub mod cat;
    pub mod diff;
    pub mod edit;
    pub mod init;
    pub mod status;
    pub mod update;
    // ... etc
}

// Apply command example
pub fn apply::run(
    source_dir: &Path,
    dest_dir: &Path,
    filter_files: &[PathBuf],
    options: &ApplyOptions,
    config: &Config,
) -> Result<ApplyStats>;

pub struct ApplyOptions {
    pub dry_run: bool,
    pub force: bool,
    pub interactive: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

pub struct ApplyStats {
    pub total: usize,
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}
```

**Interactive TUI**:

```rust
// Conflict resolution UI
pub struct ConflictViewer {
    diff_viewer: DiffViewer,
    preview_pane: PreviewPane,
    prompt: ConflictPrompt,
}

pub enum ConflictAction {
    Overwrite,  // Replace destination with target
    Skip,       // Keep destination unchanged
    Diff,       // Show detailed diff
    Preview,    // Show file content
    Quit,       // Cancel operation
}
```

**Dependencies**:
- All other guisu crates
- `clap`: CLI parsing (v4.5+)
- `ratatui`: Terminal UI
- `git2`: Git operations
- `miette`: Beautiful error reporting
- `indicatif`: Progress bars

**Lines of Code**: ~10,000

**Design Notes**:
- CLI and library in same crate (dual-use)
- Commands are separate modules (easy to add new commands)
- Interactive mode uses ratatui (beautiful TUI)
- Git operations use git2 (no git binary dependency)
- Error handling with miette (rich error messages)

---

### Dependency Graph

**Full Dependency Graph**:

```
guisu-cli
    ├── guisu-engine
    │   ├── guisu-core
    │   ├── guisu-crypto
    │   │   └── guisu-core
    │   ├── guisu-template
    │   │   ├── guisu-core
    │   │   ├── guisu-crypto
    │   │   └── guisu-vault
    │   ├── guisu-config
    │   │   ├── guisu-core
    │   │   ├── guisu-crypto
    │   │   └── guisu-template
    │   └── guisu-vault
    ├── guisu-template (direct)
    ├── guisu-config (direct)
    ├── guisu-crypto (direct)
    └── guisu-core (direct)
```

**Verification**:
```bash
# Check for circular dependencies
cargo tree --workspace --no-dedupe
# Should show clean tree with no cycles
```

---

### Cross-Cutting Concerns

#### Error Handling

**Strategy**: Two-tier error handling

1. **Application Errors** (CLI): `anyhow::Error`
   - Rich context with `.context()`
   - Beautiful display with `miette`
   - Backtrace support

2. **Library Errors**: `thiserror` for each crate
   - Typed errors with `#[derive(Error)]`
   - Conversion to `anyhow::Error` at boundaries

**Example**:

```rust
// Library (guisu-engine)
#[derive(Error, Debug)]
pub enum Error {
    #[error("Template error at {location}: {message}")]
    Render { location: String, message: String },

    #[error("Failed to {operation} file {path}: {source}")]
    Io {
        operation: String,
        path: String,
        #[source]
        source: std::io::Error,
    },
}

// Application (guisu-cli)
fn run() -> anyhow::Result<()> {
    engine.process()
        .context("Failed to process dotfiles")?;
    Ok(())
}
```

#### Logging

**Strategy**: Structured logging with `tracing`

```rust
use tracing::{info, warn, error, debug, trace};

#[instrument(skip(content))]
fn process_file(path: &Path, content: &[u8]) -> Result<()> {
    debug!(path = %path.display(), size = content.len(), "Processing file");
    // ...
    info!(path = %path.display(), "File processed successfully");
    Ok(())
}
```

**Levels**:
- `TRACE`: Detailed internals
- `DEBUG`: Development debugging
- `INFO`: User-visible progress
- `WARN`: Recoverable issues
- `ERROR`: Failures

#### Testing

**Strategy**: Multi-level testing

1. **Unit Tests**: In each crate
   ```rust
   #[cfg(test)]
   mod tests {
       #[test]
       fn test_parse_attributes() { }
   }
   ```

2. **Integration Tests**: In `tests/` directory
   ```rust
   #[test]
   fn test_apply_workflow() {
       // End-to-end test
   }
   ```

3. **Doctests**: In documentation
   ```rust
   /// # Example
   /// ```
   /// # use guisu_core::AbsPath;
   /// let path = AbsPath::new("/home/user")?;
   /// ```
   ```

#### Configuration

**Layered Configuration**:

1. Defaults (in code)
2. Source config (.guisu.toml)
3. Platform variables (.guisu/{platform}/*.yaml)
5. Environment variables (GUISU_*)
6. CLI flags

**Priority**: Later layers override earlier layers

---

<a name="中文"></a>

## 中文

本文档详细介绍了 Guisu 的容器架构 - 组成系统的 7 个 crate 及其职责。

### 目录

- [架构概览](#架构概览)
- [依赖层级](#依赖层级)
- [Crate 详情](#crate-详情)
  - [层级 0：基础](#层级-0基础)
  - [层级 1：基础服务](#层级-1基础服务)
  - [层级 2：处理](#层级-2处理)
  - [层级 3：引擎](#层级-3引擎)
  - [层级 4：接口](#层级-4接口)
- [依赖图](#依赖图)
- [横切关注点](#横切关注点)

---

### 架构概览

Guisu 组织为一个包含 7 个 crate 的 Cargo 工作空间，遵循严格的分层架构：

```
crates/
├── core/        # 层级 0：基础类型
├── crypto/      # 层级 1：Age 加密
├── vault/       # 层级 1：密码管理器
├── template/    # 层级 2：模板渲染
├── config/      # 层级 2：配置
├── engine/      # 层级 3：状态管理
└── cli/         # 层级 4：用户界面
```

**关键原则：**

1. **单向依赖**：高层依赖低层，永不反向
2. **无循环依赖**：清晰的依赖图
3. **单一职责**：每个 crate 都有一个明确的目的
4. **最小公共 API**：只暴露必要的内容

---

### 依赖层级

（图表与英文版相同）

---

### Crate 详情

#### 层级 0：基础

##### `guisu-core`

**目的**：所有 crate 共享的基础类型和工具。

**职责**：
- 类型安全的路径类型（`AbsPath`、`RelPath`、`SourceRelPath`）
- 平台检测（`Platform`、`CURRENT_PLATFORM`）
- 核心错误类型
- 通用工具

**依赖**：无（仅标准库）

**代码行数**：~300

**设计说明**：
- NewType 模式在编译时防止混淆绝对/相对路径
- 平台检测在编译时完成一次
- 路径类型零运行时开销

---

#### 层级 1：基础服务

##### `guisu-crypto`

**目的**：文件和内联值的 Age 加密和解密。

**职责**：
- 加载 age 身份（SSH 密钥、age 密钥）
- 解密 age 加密文件
- 为多个接收者加密文件
- 配置值的内联加密

**依赖**：
- `guisu-core`：路径类型
- `age`：加密库（v0.11+）

**代码行数**：~500

**设计说明**：
- 支持 SSH 密钥和原生 age 密钥
- 多身份（尝试每个直到一个有效）
- 多接收者（加密一次，用任何密钥解密）
- Armor 格式用于人类可读的加密数据

---

##### `guisu-vault`

**目的**：密码管理器集成以获取密钥。

**职责**：
- 密钥提供者的抽象接口
- Bitwarden CLI 集成（bw、rbw、bws）
- 昂贵 vault 操作的缓存层
- 命令执行和 JSON 解析

**依赖**：
- 可选依赖 `guisu-core`（可以独立使用）

**代码行数**：~400

**设计说明**：
- 基于 trait 的设计允许轻松添加新提供者
- 缓存防止冗余 vault 操作（昂贵）
- 可选提供者的特性标志（`bw`、`rbw`、`bws`）
- 基于 JSON 的 API（vault CLI 返回 JSON）

---

#### 层级 2：处理

##### `guisu-template`

**目的**：使用 minijinja（兼容 Jinja2）进行模板渲染。

**职责**：
- 初始化 minijinja 环境
- 注册模板函数（约 30 个函数）
- 提供模板上下文
- 平台感知模板加载
- 带错误处理的模板渲染

**注册函数**（约 30 个）：

| 类别 | 函数 |
|------|------|
| 系统 | `os()`、`arch()`、`hostname()`、`username()`、`home_dir()` |
| 环境 | `env(name)`、`lookPath(cmd)` |
| 路径 | `joinPath(parts...)` |
| Bitwarden | `bitwarden(args)`、`bitwardenFields(args)`、`bitwardenAttachment()`、`bitwardenSecrets()` |
| 模板 | `include(name)`、`includeTemplate(name)` |
| 加密 | `decrypt(value)`、`encrypt(value)` |
| 字符串 | `regexMatch()`、`regexReplaceAll()`、`split()`、`join()`、`quote`、`trim` |
| 数据格式 | `toJson`、`fromJson`、`toToml`、`fromToml` |

**依赖**：
- `guisu-core`：平台检测、路径类型
- `guisu-crypto`：解密/加密函数
- `guisu-vault`：Bitwarden 集成
- `minijinja`：模板引擎（v2.12+）

**代码行数**：~1,200

**设计说明**：
- 平台感知模板加载（搜索 `darwin/` 然后回退）
- 兼容 Jinja2 的语法（大多数开发者熟悉）
- 丰富的函数库（30 个函数 vs chezmoi 的 200+ 个 - 有增长空间）
- 基于 Arc 的身份共享（无克隆）

---

##### `guisu-config`

**目的**：配置加载、解析和管理。

**职责**：
- 加载 `.guisu.toml` 或 `.guisu.toml.j2`
- 解析 TOML 配置
- 加载平台特定变量
- 使用智能基于部分的合并来合并变量
- 解析路径（相对 → 绝对）
- 向其他 crate 提供配置

**变量加载**：

```
.guisu/variables/
├── user.toml              # 全局用户变量
├── visual.toml            # 全局视觉设置
├── darwin/
│   ├── git.toml          # macOS git 设置
│   └── terminal.toml     # macOS 终端设置
└── linux/
    ├── git.toml          # Linux git 设置
    └── terminal.toml     # Linux 终端设置
```

**合并策略**：
- 首先加载全局变量
- 平台变量按部分名称覆盖
- 不同部分保持独立

**依赖**：
- `guisu-core`：路径类型、平台检测
- `guisu-crypto`：身份加载
- `guisu-template`：模板渲染（用于 `.guisu.toml.j2`）
- `serde`：序列化框架
- `toml`：TOML 解析

**代码行数**：~2,000

**设计说明**：
- 支持模板化配置（`.guisu.toml.j2`）
- 平台感知变量加载
- 智能合并（基于部分，不是全部覆盖）
- 路径解析（相对 → 绝对，`~/` → home）

---

#### 层级 3：引擎

##### `guisu-engine`

**目的**：实现三态模型的核心 dotfile 处理引擎。

**职责**：
- 源状态管理（仓库文件）
- 目标状态管理（处理后的期望状态）
- 目的地状态管理（实际文件系统）
- 持久化状态管理（redb 数据库）
- 内容处理管道
- 条目类型定义
- 文件属性解析
- 三方比较逻辑

**处理管道**：

```
1. SourceState::read(source_dir)
   - 遍历目录（使用 rayon 并行）
   - 从文件名解析属性
   - 创建 SourceEntry 对象

2. TargetState::from_source(source, processor, context)
   - 处理文件（使用 rayon 并行）
   - 解密 .age 文件
   - 渲染 .j2 模板
   - 创建 TargetEntry 对象

3. DestinationState::read(dest_dir, target)
   - 从磁盘读取实际文件
   - 创建 DestEntry 对象

4. 比较 Target ↔ Destination ↔ Database
   - 三方比较
   - 检测：同步、修改、添加、删除、冲突

5. 应用变更
   - 写入文件
   - 设置权限
   - 更新数据库
```

**依赖**：
- `guisu-core`：所有路径类型、平台检测
- `guisu-crypto`：Age 解密
- `guisu-template`：模板渲染
- `guisu-config`：配置
- `guisu-vault`：模板上下文可选
- `redb`：持久化数据库
- `rayon`：并行处理
- `walkdir`：目录遍历

**代码行数**：~4,000

**设计说明**：
- 基于 trait 的内容处理（可插拔解密器/渲染器）
- I/O 密集型操作的并行处理
- 三态模型实现复杂的变更检测
- 持久化状态跟踪内容哈希（不是完整内容）
- Bitflags 用于高效的属性存储

---

#### 层级 4：接口

##### `guisu-cli`

**目的**：命令行界面和用户交互。

**职责**：
- CLI 参数解析
- 命令实现
- 交互式 TUI（冲突解决）
- 进度报告
- 错误显示
- Git 操作
- 统计跟踪

**已实现的命令**：

```
guisu
├── age/          # 加密管理
│   ├── generate  # 生成 age 身份
│   ├── encrypt   # 加密值
│   ├── decrypt   # 解密值
│   └── recipients# 列出接收者
├── add           # 添加文件到源
├── apply         # 应用变更
├── cat           # 显示处理后的文件
├── config/       # 配置管理
│   └── get       # 获取配置值
├── diff          # 显示差异
├── edit          # 编辑源文件
├── hooks/        # Hook 管理（未来）
├── ignored/      # 忽略模式
├── info          # 系统信息
├── init          # 初始化仓库
├── status        # 显示文件状态
├── templates/    # 模板管理
│   └── execute   # 执行模板
├── update        # 拉取并应用
└── variables     # 显示模板变量
```

**依赖**：
- 所有其他 guisu crate
- `clap`：CLI 解析（v4.5+）
- `ratatui`：终端 UI
- `git2`：Git 操作
- `miette`：精美错误报告
- `indicatif`：进度条

**代码行数**：~10,000

**设计说明**：
- CLI 和库在同一个 crate 中（双重用途）
- 命令是独立的模块（易于添加新命令）
- 交互模式使用 ratatui（精美 TUI）
- Git 操作使用 git2（无 git 二进制依赖）
- 使用 miette 进行错误处理（丰富的错误消息）

---

### 依赖图

（与英文版相同）

---

### 横切关注点

#### 错误处理

**策略**：两层错误处理

1. **应用程序错误**（CLI）：`anyhow::Error`
   - 使用 `.context()` 提供丰富上下文
   - 使用 `miette` 精美显示
   - 支持回溯

2. **库错误**：每个 crate 使用 `thiserror`
   - 使用 `#[derive(Error)]` 的类型化错误
   - 在边界转换为 `anyhow::Error`

#### 日志记录

**策略**：使用 `tracing` 进行结构化日志记录

**级别**：
- `TRACE`：详细内部信息
- `DEBUG`：开发调试
- `INFO`：用户可见进度
- `WARN`：可恢复问题
- `ERROR`：失败

#### 测试

**策略**：多层测试

1. **单元测试**：在每个 crate 中
2. **集成测试**：在 `tests/` 目录
3. **文档测试**：在文档中

#### 配置

**分层配置**：

1. 默认值（在代码中）
2. 全局配置（~/.config/guisu/config.toml）- 已弃用
3. 源配置（.guisu.toml）
4. 平台变量（.guisu/{platform}/*.yaml）
5. 环境变量（GUISU_*）
6. CLI 标志

**优先级**：后面的层覆盖前面的层
