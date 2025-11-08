# Contributing to Guisu / 为 Guisu 做贡献

[English](#english) | [中文](#中文)

---

<a name="english"></a>

## English

Thank you for your interest in contributing to Guisu! This document provides guidelines for contributing to the project.

**Important**: Guisu is currently in early development (pre-1.0). Expect significant changes and breaking API changes.

### Table of Contents

- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Code Style](#code-style)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Architecture Guidelines](#architecture-guidelines)

---

### Development Setup

**Prerequisites**:

- Rust toolchain (edition 2024)
- Git
- Optional: mise for development environment management

**Installation**:

```bash
# Clone the repository
git clone https://github.com/yourusername/guisu.git
cd guisu

# Install Rust toolchain (if using mise)
mise install

# Build the project
cargo build

# Run tests
cargo test

# Run clippy (linter)
cargo clippy --all-targets --all-features

# Format code
cargo fmt
```

**Recommended Tools**:

- `mise`: Development environment manager (see `mise.toml`)
- `pre-commit`: Git hooks for code quality
- `rust-analyzer`: IDE support
- `cargo-nextest`: Faster test runner

**Setting up pre-commit**:

```bash
# Install pre-commit hooks
pre-commit install

# Run manually
pre-commit run --all-files
```

---

### Project Structure

```
guisu/
├── crates/               # Workspace members
│   ├── core/            # Layer 0: Foundation types
│   ├── crypto/          # Layer 1: Age encryption
│   ├── vault/           # Layer 1: Password managers
│   ├── template/        # Layer 2: Template rendering
│   ├── config/          # Layer 2: Configuration
│   ├── engine/          # Layer 3: State management
│   └── cli/             # Layer 4: User interface
├── docs/                # Documentation
│   ├── architecture/    # Architecture docs
│   └── development/     # Development docs
├── Cargo.toml           # Workspace configuration
├── CLAUDE.md            # Development reference (chezmoi comparison)
├── README.md            # Project README
└── rust-toolchain.toml  # Rust edition specification
```

**Key Files**:

- `CLAUDE.md`: Comprehensive reference comparing guisu to chezmoi, must-read for contributors
- `docs/architecture/`: Detailed architecture documentation
- `.rust-analyzer.toml`: IDE configuration
- `.rustfmt.toml`: Code formatting rules

---

### Code Style

**Rust Style**:

Follow standard Rust conventions with project-specific rules:

1. **Naming**:
   - Types: `PascalCase` (e.g., `SourceState`, `TargetEntry`)
   - Functions: `snake_case` (e.g., `read_source_state`)
   - Constants: `SCREAMING_SNAKE_CASE` (e.g., `CURRENT_PLATFORM`)
   - Modules: `snake_case` (e.g., `persistent_state`)

2. **Formatting**:
   ```bash
   # Run before committing
   cargo fmt
   ```

3. **Linting**:
   ```bash
   # Fix auto-fixable issues
   cargo clippy --fix --allow-dirty

   # Check for issues
   cargo clippy --all-targets --all-features -- -D warnings
   ```

4. **Documentation**:
   - All public items must have doc comments
   - Use examples in doc comments where helpful
   - Run `cargo doc --open` to view documentation

**Example**:

```rust
/// Reads the source state from the repository directory.
///
/// This function walks the source directory in parallel using rayon,
/// parsing file attributes from filenames and creating `SourceEntry` objects.
///
/// # Arguments
///
/// * `root` - Absolute path to the source directory
///
/// # Returns
///
/// A `SourceState` containing all entries found in the directory.
///
/// # Errors
///
/// Returns an error if:
/// - The directory cannot be read
/// - File attributes cannot be parsed
/// - Any file has an unsupported type
///
/// # Example
///
/// ```no_run
/// # use guisu_core::AbsPath;
/// # use guisu_engine::SourceState;
/// let root = AbsPath::new("/home/user/.local/share/guisu")?;
/// let state = SourceState::read(root)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read(root: AbsPath) -> Result<Self> {
    // Implementation
}
```

---

### Testing

**Test Organization**:

1. **Unit Tests**: In each crate
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_parse_attributes() {
           let (attrs, name) = FileAttributes::parse_from_source(".bashrc.j2", None).unwrap();
           assert!(attrs.is_template());
           assert_eq!(name, ".bashrc");
       }
   }
   ```

2. **Integration Tests**: In `crates/*/tests/`
   ```rust
   // crates/engine/tests/integration.rs
   #[test]
   fn test_apply_workflow() {
       let temp = TempDir::new().unwrap();
       // Setup test repository
       // Run apply
       // Verify results
   }
   ```

3. **Doc Tests**: In documentation
   ```rust
   /// # Example
   /// ```
   /// # use guisu_core::AbsPath;
   /// let path = AbsPath::new("/home/user")?;
   /// # Ok::<(), anyhow::Error>(())
   /// ```
   ```

**Running Tests**:

```bash
# All tests
cargo test

# Specific crate
cargo test -p guisu-engine

# Specific test
cargo test test_parse_attributes

# With output
cargo test -- --nocapture

# Parallel test runner (faster)
cargo nextest run
```

**Test Coverage**:

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage
```

---

### Pull Request Process

1. **Fork and Branch**:
   ```bash
   # Fork on GitHub, then:
   git clone https://github.com/yourusername/guisu.git
   cd guisu
   git checkout -b feature/my-feature
   ```

2. **Make Changes**:
   - Follow code style guidelines
   - Add tests for new functionality
   - Update documentation if needed
   - Run `cargo fmt` and `cargo clippy`

3. **Commit**:
   ```bash
   # Conventional commit format
   git commit -m "feat: add script execution system"
   git commit -m "fix: resolve template rendering bug"
   git commit -m "docs: update architecture diagrams"
   ```

   **Commit Types**:
   - `feat`: New feature
   - `fix`: Bug fix
   - `docs`: Documentation changes
   - `style`: Code style changes (formatting)
   - `refactor`: Code refactoring
   - `perf`: Performance improvements
   - `test`: Adding or fixing tests
   - `chore`: Build process, tools, dependencies

4. **Push and Create PR**:
   ```bash
   git push origin feature/my-feature
   # Create PR on GitHub
   ```

5. **PR Requirements**:
   - Clear description of changes
   - Reference related issues
   - All tests passing
   - No clippy warnings
   - Code formatted with `cargo fmt`
   - Documentation updated if needed

**PR Template**:

```markdown
## Description
Brief description of changes

## Motivation
Why is this change needed?

## Changes
- List of changes
- Another change

## Testing
How was this tested?

## Checklist
- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] `cargo fmt` run
- [ ] `cargo clippy` passing
- [ ] All tests passing
```

---

### Architecture Guidelines

**Before Adding Features**:

1. **Research chezmoi**: Check `CLAUDE.md` and chezmoi's implementation
2. **Design First**: Discuss architecture in an issue before implementing
3. **Follow Patterns**: Use existing patterns (NewType, trait-based, etc.)
4. **Consider Performance**: Use rayon for parallelizable operations

**Layered Architecture**:

- Layer 0 (core): Only std dependencies
- Layer 1 (crypto, vault): Can depend on core
- Layer 2 (template, config): Can depend on Layer 0-1
- Layer 3 (engine): Can depend on Layer 0-2
- Layer 4 (cli): Can depend on all layers

**No Circular Dependencies**: Verify with `cargo tree`

**Error Handling**:

- Libraries: Use `thiserror` for typed errors
- CLI: Convert to `anyhow::Error` at boundaries
- Add context with `.context()`

**Example**:

```rust
// Library error (guisu-engine)
#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to read source file {path}: {source}")]
    ReadSource {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

// CLI usage (guisu-cli)
engine.read_source(&path)
    .context("Failed to read source state")?;
```

---

<a name="中文"></a>

## 中文

感谢您对为 Guisu 做贡献的兴趣！本文档提供了为项目做贡献的指南。

**重要**：Guisu 目前处于早期开发阶段（1.0 版本之前）。预期会有重大变更和破坏性 API 变更。

### 目录

- [开发环境设置](#开发环境设置)
- [项目结构](#项目结构-1)
- [代码风格](#代码风格)
- [测试](#测试-1)
- [Pull Request 流程](#pull-request-流程)
- [架构指南](#架构指南)

---

### 开发环境设置

**前提条件**：

- Rust 工具链（edition 2024）
- Git
- 可选：mise 用于开发环境管理

**安装**：

```bash
# 克隆仓库
git clone https://github.com/yourusername/guisu.git
cd guisu

# 安装 Rust 工具链（如果使用 mise）
mise install

# 构建项目
cargo build

# 运行测试
cargo test

# 运行 clippy（代码检查工具）
cargo clippy --all-targets --all-features

# 格式化代码
cargo fmt
```

**推荐工具**：

- `mise`：开发环境管理器（见 `mise.toml`）
- `pre-commit`：代码质量的 Git hook
- `rust-analyzer`：IDE 支持
- `cargo-nextest`：更快的测试运行器

---

### 项目结构

```
guisu/
├── crates/               # 工作空间成员
│   ├── core/            # 层级 0：基础类型
│   ├── crypto/          # 层级 1：Age 加密
│   ├── vault/           # 层级 1：密码管理器
│   ├── template/        # 层级 2：模板渲染
│   ├── config/          # 层级 2：配置
│   ├── engine/          # 层级 3：状态管理
│   └── cli/             # 层级 4：用户界面
├── docs/                # 文档
│   ├── architecture/    # 架构文档
│   └── development/     # 开发文档
├── Cargo.toml           # 工作空间配置
├── CLAUDE.md            # 开发参考（chezmoi 对比）
├── README.md            # 项目 README
└── rust-toolchain.toml  # Rust edition 规范
```

**关键文件**：

- `CLAUDE.md`：比较 guisu 和 chezmoi 的综合参考，贡献者必读
- `docs/architecture/`：详细的架构文档
- `.rust-analyzer.toml`：IDE 配置
- `.rustfmt.toml`：代码格式化规则

---

### 代码风格

**Rust 风格**：

遵循标准 Rust 约定和项目特定规则：

1. **命名**：
   - 类型：`PascalCase`（例如，`SourceState`、`TargetEntry`）
   - 函数：`snake_case`（例如，`read_source_state`）
   - 常量：`SCREAMING_SNAKE_CASE`（例如，`CURRENT_PLATFORM`）
   - 模块：`snake_case`（例如，`persistent_state`）

2. **格式化**：
   ```bash
   # 提交前运行
   cargo fmt
   ```

3. **代码检查**：
   ```bash
   # 修复可自动修复的问题
   cargo clippy --fix --allow-dirty

   # 检查问题
   cargo clippy --all-targets --all-features -- -D warnings
   ```

4. **文档**：
   - 所有公共项必须有文档注释
   - 在文档注释中使用示例（如果有帮助）
   - 运行 `cargo doc --open` 查看文档

---

### 测试

**测试组织**：

1. **单元测试**：在每个 crate 中
2. **集成测试**：在 `crates/*/tests/` 中
3. **文档测试**：在文档中

**运行测试**：

```bash
# 所有测试
cargo test

# 特定 crate
cargo test -p guisu-engine

# 特定测试
cargo test test_parse_attributes

# 带输出
cargo test -- --nocapture

# 并行测试运行器（更快）
cargo nextest run
```

---

### Pull Request 流程

1. **Fork 和分支**：
   ```bash
   # 在 GitHub 上 fork，然后：
   git clone https://github.com/yourusername/guisu.git
   cd guisu
   git checkout -b feature/my-feature
   ```

2. **进行更改**：
   - 遵循代码风格指南
   - 为新功能添加测试
   - 如需要更新文档
   - 运行 `cargo fmt` 和 `cargo clippy`

3. **提交**：
   ```bash
   # 约定式提交格式
   git commit -m "feat: 添加脚本执行系统"
   git commit -m "fix: 解决模板渲染 bug"
   git commit -m "docs: 更新架构图"
   ```

   **提交类型**：
   - `feat`：新功能
   - `fix`：Bug 修复
   - `docs`：文档更改
   - `style`：代码风格更改（格式化）
   - `refactor`：代码重构
   - `perf`：性能改进
   - `test`：添加或修复测试
   - `chore`：构建过程、工具、依赖

4. **推送并创建 PR**：
   ```bash
   git push origin feature/my-feature
   # 在 GitHub 上创建 PR
   ```

5. **PR 要求**：
   - 清晰的更改描述
   - 引用相关 issue
   - 所有测试通过
   - 无 clippy 警告
   - 使用 `cargo fmt` 格式化代码
   - 如需要更新文档

---

### 架构指南

**添加功能之前**：

1. **研究 chezmoi**：检查 `CLAUDE.md` 和 chezmoi 的实现
2. **首先设计**：在实现之前在 issue 中讨论架构
3. **遵循模式**：使用现有模式（NewType、基于 trait 等）
4. **考虑性能**：对可并行化的操作使用 rayon

**分层架构**：

- 层级 0（core）：仅 std 依赖
- 层级 1（crypto、vault）：可以依赖 core
- 层级 2（template、config）：可以依赖层级 0-1
- 层级 3（engine）：可以依赖层级 0-2
- 层级 4（cli）：可以依赖所有层级

**无循环依赖**：使用 `cargo tree` 验证

**错误处理**：

- 库：使用 `thiserror` 进行类型化错误
- CLI：在边界转换为 `anyhow::Error`
- 使用 `.context()` 添加上下文
