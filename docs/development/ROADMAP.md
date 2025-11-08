# Guisu Development Roadmap / Guisu 开发路线图

[English](#english) | [中文](#中文)

---

<a name="english"></a>

## English

This document outlines the development roadmap for Guisu, organized by priority and target quarters.

**Current Status**: Early Development (v0.1.x)
**Target v1.0**: Q4 2025

### Priority Legend

- **P0 (Critical)**: Blocking v1.0 release
- **P1 (High)**: Important for feature parity
- **P2 (Medium)**: Nice to have
- **P3 (Low)**: Future enhancements

---

### Q1 2025 (January - March): Core Features

#### P0: Script Execution System

**Status**: Not Started
**Effort**: 4-6 weeks
**Owner**: TBD

**Description**: Implement the script execution system, the most critical missing feature compared to chezmoi.

**Tasks**:
- [ ] Design ScriptType enum and attributes parsing
- [ ] Add Script entry type to TargetEntry
- [ ] Extend persistent state for script tracking
- [ ] Create ScriptExecutor module
  - [ ] Implement `run_before_*` support
  - [ ] Implement `run_after_*` support
  - [ ] Implement `run_once_*` support
  - [ ] Implement `run_onchange_*` support
- [ ] Integrate into apply pipeline
- [ ] Add configuration options
- [ ] Write comprehensive tests
- [ ] Update documentation

**Acceptance Criteria**:
- All four script types working correctly
- State tracking prevents re-running `run_once_*` scripts
- Content hash tracking for `run_onchange_*` scripts
- Scripts execute in correct order (numeric prefixes)
- Errors handled gracefully

**Related Issues**: #TBD

---

#### P0: Doctor Command

**Status**: Not Started
**Effort**: 1 week
**Owner**: TBD

**Description**: Implement system diagnostics command to verify installation and configuration.

**Tasks**:
- [ ] Check guisu version
- [ ] Verify configuration file
- [ ] Check age identity files
- [ ] Verify git repository
- [ ] Test vault provider availability
- [ ] Check template engine
- [ ] Verify persistent state database
- [ ] Display summary report

**Acceptance Criteria**:
- Clear diagnostic output
- Identifies common issues
- Suggests fixes for problems

---

#### P1: Template Functions Expansion (Phase 1)

**Status**: Not Started
**Effort**: 2-3 weeks
**Owner**: TBD

**Description**: Add 20 most commonly used template functions from chezmoi.

**Tasks**:
- [ ] File operations (5 functions)
  - [ ] `include(path)` - Include file contents
  - [ ] `includeTemplate(path)` - Include and render
  - [ ] `readFile(path)` - Read arbitrary file
  - [ ] `glob(pattern)` - Match file patterns
  - [ ] `stat(path)` - File information
- [ ] Data formats (4 functions)
  - [ ] `toJson(value)` - Convert to JSON
  - [ ] `fromJson(string)` - Parse JSON
  - [ ] `toToml(value)` - Convert to TOML
  - [ ] `fromToml(string)` - Parse TOML
- [ ] String processing (5 functions)
  - [ ] `regexMatch(pattern, string)` - Regex matching
  - [ ] `regexReplaceAll(pattern, replacement, string)` - Regex replacement
  - [ ] `split(separator, string)` - String splitting
  - [ ] `join(separator, array)` - String joining
  - [ ] `base64Encode/Decode(string)` - Base64 encoding/decoding
- [ ] Encryption (4 functions)
  - [ ] `sha256sum(content)` - Compute SHA256
  - [ ] `sha512sum(content)` - Compute SHA512
  - [ ] `md5sum(content)` - Compute MD5 (for compatibility)
  - [ ] `encryptFile/decryptFile(path)` - File encryption helpers

**Acceptance Criteria**:
- All functions working correctly
- Comprehensive tests
- Documentation with examples

---

### Q2 2025 (April - June): High-Value Features

#### P0: External Resources System

**Status**: Not Started
**Effort**: 3-4 weeks
**Owner**: TBD

**Description**: Implement `.guisu.external.toml` for downloading and managing external files/archives.

**Tasks**:
- [ ] Design ExternalConfig data structure
- [ ] Implement HTTP client with caching
- [ ] Add archive extraction support (tar.gz, zip)
- [ ] Implement refresh logic (time-based)
- [ ] Add checksum verification
- [ ] Add file type support
- [ ] Add archive type support with filters
- [ ] Integrate into apply pipeline
- [ ] Add configuration examples
- [ ] Write tests

**Acceptance Criteria**:
- Can download files from URLs
- Can extract archives with filtering
- Respects refresh periods
- Verifies checksums
- Updates persistent state

**Example Usage**:
```toml
# .guisu.external.toml
[".oh-my-zsh"]
    type = "archive"
    url = "https://github.com/ohmyzsh/ohmyzsh/archive/master.tar.gz"
    stripComponents = 1
    refreshPeriod = "168h"

["bin/kubectl"]
    type = "file"
    url = "https://dl.k8s.io/release/v1.28.0/bin/linux/amd64/kubectl"
    executable = true
```

---

#### P0: Modify File Type

**Status**: Not Started
**Effort**: 2 weeks
**Owner**: TBD

**Description**: Implement `modify_*` prefix for in-place file modification.

**Tasks**:
- [ ] Add Modify entry type
- [ ] Design modify script execution
- [ ] Set environment variables for target file
- [ ] Implement modify executor
- [ ] Add to apply pipeline
- [ ] Write tests
- [ ] Add documentation

**Acceptance Criteria**:
- Can modify existing files
- Script receives target file path
- Changes applied atomically
- Errors handled gracefully

---

#### P1: Password Manager Expansion

**Status**: Not Started
**Effort**: 4 weeks (1 week per provider)
**Owner**: TBD

**Description**: Add support for major password managers.

**Priority Order**:
1. **1Password** (High - very popular)
   - CLI: `op`
   - Functions: `onepasswordRead`, `onepasswordDocument`
2. **Pass** (High - Unix standard)
   - CLI: `pass`
   - Functions: `pass(path)`
3. **System Keychain** (Medium)
   - macOS: `security`
   - Linux: `secret-tool`
   - Functions: `keychain(service, account)`
4. **HashiCorp Vault** (Medium - enterprise)
   - CLI: `vault`
   - Functions: `vault(path)`

**Tasks per Provider**:
- [ ] Implement provider trait
- [ ] Add CLI command execution
- [ ] Parse JSON responses
- [ ] Add caching support
- [ ] Register template functions
- [ ] Add feature flag
- [ ] Write tests
- [ ] Document usage

---

#### P1: Unmanaged Command

**Status**: Not Started
**Effort**: 1 week
**Owner**: TBD

**Description**: List files in destination that aren't managed by guisu.

**Tasks**:
- [ ] Build set of managed paths
- [ ] Walk destination directory
- [ ] Filter managed files
- [ ] Apply ignore patterns
- [ ] Display unmanaged files
- [ ] Add filtering options
- [ ] Write tests

---

#### P1: Re-add Command

**Status**: Not Started
**Effort**: 1 week
**Owner**: TBD

**Description**: Update source from modified destination files.

**Tasks**:
- [ ] Find files that differ from state
- [ ] Compare with persistent database
- [ ] Update source files
- [ ] Preserve attributes
- [ ] Handle encryption
- [ ] Handle templates
- [ ] Write tests

---

### Q3 2025 (July - September): Quality & Completeness

#### P1: Template Functions Expansion (Phase 2)

**Status**: Not Started
**Effort**: 3-4 weeks
**Owner**: TBD

**Description**: Add remaining commonly used template functions.

**Tasks**:
- [ ] Git integration (5 functions)
  - [ ] `gitHead()` - Current commit
  - [ ] `gitBranch()` - Current branch
  - [ ] `gitStatus()` - Working tree status
  - [ ] `gitTag()` - Latest tag
  - [ ] `gitRemote()` - Remote URL
- [ ] System info (5 functions)
  - [ ] `kernel()` - Kernel version
  - [ ] `kernelVersion()` - Kernel version number
  - [ ] `osRelease()` - OS release info
  - [ ] `cpuCores()` - CPU core count
  - [ ] `timezone()` - System timezone
- [ ] Advanced filters (10 functions)
  - [ ] `indent(n)` - Indent text
  - [ ] `nindent(n)` - Indent with newline
  - [ ] `trim(chars)` - Trim characters
  - [ ] `replace(old, new)` - String replacement
  - [ ] `default(value)` - Default value
  - [ ] `empty()` - Check if empty
  - [ ] `list()` - Create list
  - [ ] `dict()` - Create dictionary
  - [ ] `merge()` - Merge dictionaries
  - [ ] `keys()` - Dictionary keys

---

#### P2: Archive Command

**Status**: Not Started
**Effort**: 1 week
**Owner**: TBD

**Description**: Create tar/zip archives of managed files.

**Tasks**:
- [ ] Collect managed files
- [ ] Support tar.gz format
- [ ] Support zip format
- [ ] Include/exclude filters
- [ ] Preserve permissions
- [ ] Write tests

---

#### P2: Verify Command

**Status**: Not Started
**Effort**: 1 week
**Owner**: TBD

**Description**: Verify all files match expected state.

**Tasks**:
- [ ] Read target state
- [ ] Read destination state
- [ ] Compare all files
- [ ] Report mismatches
- [ ] Return exit code
- [ ] Write tests

---

#### P2: Merge Command

**Status**: Not Started
**Effort**: 2 weeks
**Owner**: TBD

**Description**: Three-way merge tool for resolving conflicts.

**Tasks**:
- [ ] Implement three-way merge algorithm
- [ ] Create merge UI
- [ ] Handle conflicts
- [ ] Update files
- [ ] Write tests

---

### Q4 2025 (October - December): Polish & v1.0

#### P0: Documentation

**Status**: In Progress
**Effort**: Ongoing
**Owner**: Current

**Tasks**:
- [x] Architecture documentation
- [x] C4 model diagrams
- [x] Data flow documentation
- [x] Contributing guide
- [x] Roadmap
- [ ] User guide
- [ ] Tutorial
- [ ] API documentation (rustdoc)
- [ ] Migration guide (from chezmoi)

---

#### P0: Testing & Quality

**Status**: Ongoing
**Effort**: Ongoing
**Owner**: All

**Tasks**:
- [ ] Increase unit test coverage (target: 80%)
- [ ] Add integration tests
- [ ] Add property-based tests
- [ ] Performance benchmarks
- [ ] Memory profiling
- [ ] Security audit
- [ ] Fuzzing

---

#### P1: Windows Support

**Status**: Not Started
**Effort**: 2-3 weeks
**Owner**: TBD

**Description**: Improve Windows compatibility.

**Tasks**:
- [ ] Test on Windows
- [ ] Handle Windows paths
- [ ] Handle Windows permissions
- [ ] Handle line endings
- [ ] Add Windows-specific ignores
- [ ] CI/CD for Windows
- [ ] Documentation for Windows users

---

#### P2: Performance Optimization

**Status**: Ongoing
**Effort**: 2-3 weeks
**Owner**: TBD

**Tasks**:
- [ ] Profile hot paths
- [ ] Optimize file I/O
- [ ] Optimize template rendering
- [ ] Optimize database operations
- [ ] Reduce memory allocations
- [ ] Benchmark against chezmoi
- [ ] Document performance characteristics

---

#### P3: Create File Type

**Status**: Not Started
**Effort**: 1 week
**Owner**: TBD

**Description**: Implement `create_*` prefix for create-only files.

**Tasks**:
- [ ] Add Create entry type
- [ ] Check if file exists
- [ ] Create only if missing
- [ ] Skip if exists
- [ ] Write tests

---

### Future (Post v1.0)

#### Plugin System

**Status**: Research
**Effort**: TBD
**Owner**: TBD

**Description**: WASM-based plugin system for extensibility.

**Possible Features**:
- Custom template functions
- Custom entry types
- Custom vault providers
- Custom template loaders

---

#### Distributed State

**Status**: Research
**Effort**: TBD
**Owner**: TBD

**Description**: Multi-machine state synchronization.

**Research Topics**:
- Conflict-free replicated data types (CRDTs)
- Eventually consistent state
- P2P synchronization
- Central state server

---

#### GUI/TUI

**Status**: Research
**Effort**: TBD
**Owner**: TBD

**Description**: Graphical user interface or full-screen TUI.

**Possible Features**:
- File browser
- Diff viewer
- Configuration editor
- Template editor

---

### Release Schedule

| Version | Target Date | Key Features |
|---------|------------|--------------|
| v0.2.0 | Q1 2025 | Script execution, doctor command |
| v0.3.0 | Q2 2025 | External resources, modify files, 1Password |
| v0.4.0 | Q3 2025 | Remaining template functions, more commands |
| v0.5.0 | Q4 2025 | Polish, testing, documentation |
| v1.0.0 | Q4 2025 | Stable release |

---

### How to Contribute

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

**High-Impact Areas**:
1. Script execution system (P0)
2. Password manager integrations (P1)
3. Template functions (P1)
4. Testing and documentation (P0)

**Good First Issues**:
- Add template functions (start with simple ones)
- Improve error messages
- Add tests
- Fix documentation typos

---

<a name="中文"></a>

## 中文

本文档概述了 Guisu 的开发路线图，按优先级和目标季度组织。

**当前状态**：早期开发（v0.1.x）
**目标 v1.0**：2025 年第四季度

### 优先级图例

- **P0（关键）**：阻塞 v1.0 发布
- **P1（高）**：功能平价的重要内容
- **P2（中）**：最好有
- **P3（低）**：未来增强

---

### 2025 年第一季度（1-3 月）：核心功能

#### P0：脚本执行系统

**状态**：未开始
**工作量**：4-6 周
**负责人**：待定

**描述**：实现脚本执行系统，与 chezmoi 相比最关键的缺失功能。

**任务**：
- [ ] 设计 ScriptType 枚举和属性解析
- [ ] 将 Script 条目类型添加到 TargetEntry
- [ ] 扩展脚本跟踪的持久化状态
- [ ] 创建 ScriptExecutor 模块
  - [ ] 实现 `run_before_*` 支持
  - [ ] 实现 `run_after_*` 支持
  - [ ] 实现 `run_once_*` 支持
  - [ ] 实现 `run_onchange_*` 支持
- [ ] 集成到 apply 管道
- [ ] 添加配置选项
- [ ] 编写全面的测试
- [ ] 更新文档

**验收标准**：
- 所有四种脚本类型正常工作
- 状态跟踪防止重新运行 `run_once_*` 脚本
- `run_onchange_*` 脚本的内容哈希跟踪
- 脚本按正确顺序执行（数字前缀）
- 优雅地处理错误

---

#### P0：Doctor 命令

**状态**：未开始
**工作量**：1 周
**负责人**：待定

**描述**：实现系统诊断命令以验证安装和配置。

**任务**：
- [ ] 检查 guisu 版本
- [ ] 验证配置文件
- [ ] 检查 age 身份文件
- [ ] 验证 git 仓库
- [ ] 测试 vault 提供者可用性
- [ ] 检查模板引擎
- [ ] 验证持久化状态数据库
- [ ] 显示摘要报告

---

#### P1：模板函数扩展（第一阶段）

**状态**：未开始
**工作量**：2-3 周
**负责人**：待定

**描述**：从 chezmoi 添加 20 个最常用的模板函数。

**任务**：（与英文版相同）

---

### 2025 年第二季度（4-6 月）：高价值功能

#### P0：外部资源系统

**状态**：未开始
**工作量**：3-4 周
**负责人**：待定

**描述**：实现 `.guisu.external.toml` 用于下载和管理外部文件/存档。

---

#### P0：修改文件类型

**状态**：未开始
**工作量**：2 周
**负责人**：待定

**描述**：实现 `modify_*` 前缀用于就地文件修改。

---

#### P1：密码管理器扩展

**状态**：未开始
**工作量**：4 周（每个提供者 1 周）
**负责人**：待定

**描述**：添加对主要密码管理器的支持。

**优先顺序**：
1. **1Password**（高 - 非常流行）
2. **Pass**（高 - Unix 标准）
3. **系统钥匙串**（中）
4. **HashiCorp Vault**（中 - 企业）

---

### 2025 年第三季度（7-9 月）：质量和完整性

#### P1：模板函数扩展（第二阶段）

**状态**：未开始
**工作量**：3-4 周
**负责人**：待定

**描述**：添加剩余的常用模板函数。

---

### 2025 年第四季度（10-12 月）：完善和 v1.0

#### P0：文档

**状态**：进行中
**工作量**：持续
**负责人**：当前

**任务**：
- [x] 架构文档
- [x] C4 模型图
- [x] 数据流文档
- [x] 贡献指南
- [x] 路线图
- [ ] 用户指南
- [ ] 教程
- [ ] API 文档（rustdoc）
- [ ] 迁移指南（从 chezmoi）

---

### 发布计划

| 版本 | 目标日期 | 关键功能 |
|------|---------|---------|
| v0.2.0 | 2025 年第一季度 | 脚本执行、doctor 命令 |
| v0.3.0 | 2025 年第二季度 | 外部资源、修改文件、1Password |
| v0.4.0 | 2025 年第三季度 | 剩余模板函数、更多命令 |
| v0.5.0 | 2025 年第四季度 | 完善、测试、文档 |
| v1.0.0 | 2025 年第四季度 | 稳定版本 |

---

### 如何贡献

详细指南请参见 [CONTRIBUTING.md](CONTRIBUTING.md)。

**高影响领域**：
1. 脚本执行系统（P0）
2. 密码管理器集成（P1）
3. 模板函数（P1）
4. 测试和文档（P0）

**适合新手的问题**：
- 添加模板函数（从简单的开始）
- 改进错误消息
- 添加测试
- 修复文档拼写错误
