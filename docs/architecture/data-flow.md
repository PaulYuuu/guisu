# Data Flow Architecture / 数据流架构

[English](#english) | [中文](#中文)

---

<a name="english"></a>

## English

This document describes the data flow through Guisu for various operations, showing how data moves through the system components.

### Table of Contents

- [Apply Command Flow](#apply-command-flow)
- [Init Command Flow](#init-command-flow)
- [Add Command Flow](#add-command-flow)
- [Update Command Flow](#update-command-flow)
- [Edit Command Flow](#edit-command-flow)
- [Template Rendering Flow](#template-rendering-flow)
- [Encryption/Decryption Flow](#encryptiondecryption-flow)

---

### Apply Command Flow

The `guisu apply` command is the core operation, applying all changes from source to destination.

**Complete Flow Diagram**:

```
[User Command: guisu apply]
         │
         ▼
┌────────────────────────┐
│ 1. Parse CLI Arguments │
│  - Interactive mode?   │
│  - Dry run?            │
│  - Include/exclude     │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Load Configuration  │
│  - .guisu.toml         │
│  - Platform variables  │
│  - Merge configs       │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Load Age Identities │
│  - Read key files      │
│  - Parse SSH keys      │
│  - Store in Arc        │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 4. Create Engine       │
│  - Template engine     │
│  - Build context       │
│  - Initialize vault    │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 5. Read Source State   │
│  - Walk directory (||) │
│  - Parse attributes    │
│  - Create SourceEntry  │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 6. Build Target State  │
│  - Process files (||)  │
│  - Decrypt .age        │
│  - Render .j2          │
│  - Create TargetEntry  │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 7. Open Database       │
│  - Load persistent     │
│    state (redb)        │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 8. For Each Entry      │◄─────┐
└────────┬───────────────┘      │
         │                      │
         ▼                      │
┌────────────────────────┐      │
│ 9. Read Destination    │      │
│  - Check if exists     │      │
│  - Read content        │      │
│  - Get metadata        │      │
└────────┬───────────────┘      │
         │                      │
         ▼                      │
┌────────────────────────┐      │
│ 10. Load DB State      │      │
│  - Get last hash       │      │
│  - Get last mode       │      │
└────────┬───────────────┘      │
         │                      │
         ▼                      │
┌────────────────────────┐      │
│ 11. Three-Way Compare  │      │
│  - Target vs Dest      │      │
│  - Dest vs DB          │      │
│  - Determine status    │      │
└────────┬───────────────┘      │
         │                      │
         ▼                      │
    ┌────────┐                 │
    │ Status?│                 │
    └────┬───┘                 │
         │                      │
    ┌────┼─────────────┐       │
    │    │      │      │       │
    ▼    ▼      ▼      ▼       │
 Synced Added Modified Conflict│
    │    │      │      │       │
    │    │      │      │       │
    ▼    │      │      │       │
  Skip   │      │      │       │
    │    │      │      │       │
    └────┼──────┼──────┘       │
         │      │              │
         ▼      ▼              │
    ┌────────────────┐         │
    │ Interactive?   │         │
    └────┬───────────┘         │
         │                      │
    ┌────┼────┐                │
    │    │    │                │
    ▼    ▼    │                │
   Yes   No   │                │
    │    │    │                │
    ▼    │    │                │
┌────────────────┐             │
│ 12. Show TUI   │             │
│  - Diff viewer │             │
│  - User prompt │             │
└────┬───────────┘             │
     │                         │
     ▼                         │
┌─────────────┐                │
│ User Action?│                │
└─────┬───────┘                │
      │                        │
  ┌───┼───┐                    │
  │   │   │                    │
  ▼   ▼   ▼                    │
Over Skip Quit                 │
write  │   │                   │
  │    │   │                   │
  │    │   ▼                   │
  │    │ [Cancel]              │
  │    │                       │
  │    ▼                       │
  │  Skip Entry ───────────────┤
  │                            │
  ▼                            │
┌────────────────────┐         │
│ 13. Apply Changes  │         │
│  - Write file      │         │
│  - Set permissions │         │
│  - Create symlink  │         │
└────────┬───────────┘         │
         │                     │
         ▼                     │
┌────────────────────┐         │
│ 14. Update DB      │         │
│  - Store hash      │         │
│  - Store mode      │         │
└────────┬───────────┘         │
         │                     │
         ▼                     │
┌────────────────────┐         │
│ More entries?      │─────────┘
└────────┬───────────┘
         │ No
         ▼
┌────────────────────┐
│ 15. Show Stats     │
│  - Files added     │
│  - Files modified  │
│  - Files skipped   │
│  - Errors          │
└────────┬───────────┘
         │
         ▼
    [Complete]
```

**Key Decision Points**:

1. **File Status** (step 11):
   - **Synced**: No action needed
   - **Added**: New file, create it
   - **Modified**: File changed in source, update it
   - **Conflict**: Both source and destination changed

2. **Interactive Mode** (step 12):
   - **Yes**: Show TUI, let user decide
   - **No**: Auto-apply (overwrite destination)

3. **User Action** (step 12):
   - **Overwrite**: Replace destination with target
   - **Skip**: Keep destination unchanged, don't update DB
   - **Quit**: Cancel entire operation

**Parallel Execution**:

Steps 5 and 6 use rayon for parallel processing:
- Step 5: Read all source files in parallel
- Step 6: Process all files (decrypt + render) in parallel
- Steps 8-14: Sequential (one file at a time for safety)

---

### Init Command Flow

The `guisu init` command initializes a new dotfiles repository.

**Flow**:

```
[User: guisu init username/dotfiles]
         │
         ▼
┌────────────────────────┐
│ 1. Parse Repository    │
│  - GitHub shorthand?   │
│  - Full URL?           │
│  - Local path?         │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Determine Source    │
│  - ~/.local/share/guisu│
│    (default location)  │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Check if Exists     │
└────────┬───────────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
 Exists   Not Exist
    │         │
    ▼         │
 [Error]      │
              ▼
┌────────────────────────┐
│ 4. Clone Repository    │
│  - git clone           │
│  - Use git2 library    │
│  - Handle SSH/HTTPS    │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 5. Apply Changes       │
│  - Run apply command   │
│  - Interactive mode    │
└────────┬───────────────┘
         │
         ▼
    [Complete]
```

**Options**:

```bash
guisu init owner/repo                    # GitHub shorthand
guisu init https://github.com/user/repo  # Full URL
guisu init --ssh owner/repo              # Use SSH
guisu init --depth 1 owner/repo          # Shallow clone
guisu init --no-apply owner/repo         # Don't apply after clone
```

---

### Add Command Flow

The `guisu add` command adds files from destination to source.

**Flow**:

```
[User: guisu add ~/.bashrc]
         │
         ▼
┌────────────────────────┐
│ 1. Resolve Path        │
│  - Make absolute       │
│  - Expand ~            │
│  - Verify exists       │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Compute Target Path │
│  - Strip $HOME prefix  │
│  - Result: .bashrc     │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Determine Source    │
│  - Copy to source dir  │
│  - Keep same name      │
│  - Result: .bashrc     │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 4. Encrypt?            │
└────────┬───────────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
   Yes        No
    │         │
    ▼         │
┌────────────────────┐   │
│ 5a. Encrypt File   │   │
│  - Load recipients │   │
│  - Encrypt content │   │
│  - Add .age suffix │   │
└────────┬───────────┘   │
         │               │
         └───────┬───────┘
                 │
                 ▼
┌────────────────────────┐
│ 6. Copy to Source      │
│  - Read file           │
│  - Write to source dir │
│  - Preserve metadata   │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 7. Git Add             │
│  - git add <file>      │
└────────┬───────────────┘
         │
         ▼
    [Complete]
```

**Example Transformations**:

```
Input                      Source File                Target
~/.bashrc           →      .bashrc                 →  ~/.bashrc
~/.ssh/config       →      .ssh/config             →  ~/.ssh/config
~/.ssh/id_rsa       →      .ssh/id_rsa.age         →  ~/.ssh/id_rsa (encrypted)
~/.config/nvim/init.vim →  .config/nvim/init.vim   →  ~/.config/nvim/init.vim
```

**Options**:

```bash
guisu add ~/.bashrc                  # Simple add
guisu add --encrypt ~/.ssh/id_rsa    # Add encrypted
guisu add --template ~/.gitconfig    # Add as template (.j2)
guisu add ~/.config/nvim             # Add directory recursively
```

---

### Update Command Flow

The `guisu update` command pulls latest changes and applies them.

**Flow**:

```
[User: guisu update]
         │
         ▼
┌────────────────────────┐
│ 1. Open Repository     │
│  - Find .git directory │
│  - Open with git2      │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Fetch Remote        │
│  - git fetch origin    │
│  - Get latest commits  │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Merge/Rebase        │
│  - Default: merge      │
│  - --rebase: rebase    │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 4. Check Conflicts     │
└────────┬───────────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
Conflicts  Clean
    │         │
    ▼         │
 [Error]      │
   Show       │
  files       │
              ▼
┌────────────────────────┐
│ 5. Apply Changes       │
│  - Run apply command   │
└────────┬───────────────┘
         │
         ▼
    [Complete]
```

**Options**:

```bash
guisu update                # Fetch + merge + apply
guisu update --no-apply     # Fetch + merge only
guisu update --rebase       # Use rebase instead of merge
```

---

### Edit Command Flow

The `guisu edit` command edits source files with transparent decryption/encryption.

**Flow**:

```
[User: guisu edit ~/.bashrc]
         │
         ▼
┌────────────────────────┐
│ 1. Find Source File    │
│  - Map ~/.bashrc       │
│  - Find .bashrc*       │
│  - Check .j2/.age ext  │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Check if Encrypted  │
└────────┬───────────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
   Yes        No
    │         │
    ▼         │
┌────────────────────┐   │
│ 3a. Decrypt        │   │
│  - Read .age file  │   │
│  - Decrypt content │   │
│  - Write to temp   │   │
└────────┬───────────┘   │
         │               │
         └───────┬───────┘
                 │
                 ▼
┌────────────────────────┐
│ 4. Open Editor         │
│  - Use $EDITOR         │
│  - Or config editor    │
│  - Wait for close      │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 5. Check Changes       │
│  - Compare hash        │
└────────┬───────────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
 Changed  Unchanged
    │         │
    ▼         │
┌────────────────────┐   │
│ 6a. Save Changes   │   │
│  - Encrypt if .age │   │
│  - Write to source │   │
└────────┬───────────┘   │
         │               │
         └───────┬───────┘
                 │
                 ▼
┌────────────────────────┐
│ 7. Clean Up            │
│  - Delete temp file    │
│  - Securely erase      │
└────────┬───────────────┘
         │
         ▼
    [Complete]
```

**Security Considerations**:

1. **Temporary Files**: Decrypted content written to temp file
   - Should be in secure location (/tmp with 0600)
   - Deleted after editing
   - Future: Secure erasure (overwrite before delete)

2. **Editor Security**: Editor might create backups
   - Warn user about editor backup files
   - Document how to disable (e.g., nvim: `set nobackup noswapfile`)

---

### Template Rendering Flow

**Flow**:

```
[Template File: .bashrc.j2]
         │
         ▼
┌────────────────────────┐
│ 1. Read Template       │
│  - Load from disk      │
│  - UTF-8 decode        │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Build Context       │
│  - System info         │
│  - User variables      │
│  - Config info         │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Parse Template      │
│  - minijinja parser    │
│  - Syntax validation   │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 4. Render              │
│  - Evaluate expressions│
│  - Call functions      │
│  - Apply filters       │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 5. Function Calls      │
│  - os(), hostname()    │
│  - env(), lookPath()   │
│  - bitwarden()         │
│  - decrypt()           │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 6. Output              │
│  - Rendered string     │
│  - UTF-8 bytes         │
└────────┬───────────────┘
         │
         ▼
    [Result: .bashrc]
```

**Example Template**:

```jinja2
# ~/.bashrc - Rendered on {{ os }} ({{ arch }})

# Platform-specific
{% if os == "darwin" %}
export HOMEBREW_PREFIX="/opt/homebrew"
alias ls="gls --color=auto"
{% elif os == "linux" %}
alias ls="ls --color=auto"
{% endif %}

# User configuration
export EDITOR="{{ editor }}"
export EMAIL="{{ email }}"

# Secrets
export GITHUB_TOKEN="{{ bitwarden("GitHub").login.password }}"
```

**Context Data**:

```json
{
  "os": "darwin",
  "arch": "aarch64",
  "hostname": "macbook",
  "username": "user",
  "editor": "nvim",
  "email": "user@example.com",
  "config": {
    "age": { "symmetric": true },
    "bitwarden": { "provider": "rbw" }
  }
}
```

---

### Encryption/Decryption Flow

**Encryption Flow (guisu add --encrypt)**:

```
[Plain File: ~/.ssh/id_rsa]
         │
         ▼
┌────────────────────────┐
│ 1. Read File           │
│  - Read bytes          │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Load Recipients     │
│  - From config         │
│  - Parse public keys   │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Encrypt             │
│  - age encryption      │
│  - Armor format        │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 4. Write to Source     │
│  - File: id_rsa.age    │
│  - ASCII-armored       │
└────────┬───────────────┘
         │
         ▼
    [Encrypted File]
```

**Decryption Flow (guisu apply)**:

```
[Encrypted File: id_rsa.age]
         │
         ▼
┌────────────────────────┐
│ 1. Read File           │
│  - Read ASCII armor    │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Load Identities     │
│  - From config         │
│  - Try each identity   │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Decrypt             │
│  - age decryption      │
│  - First matching key  │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 4. Write to Dest       │
│  - File: ~/.ssh/id_rsa │
│  - Mode: 0600          │
└────────┬───────────────┘
         │
         ▼
    [Plain File]
```

**Inline Encryption/Decryption**:

```
Template:
export TOKEN="{{ 'age:base64,YWdl...' | decrypt }}"

Flow:
1. Template parsed
2. decrypt filter called
3. Inline decryption: age:base64,... → plain text
4. Substituted into template
5. Result: export TOKEN="ghp_xxxxxxxxxxxx"
```

---

<a name="中文"></a>

## 中文

本文档描述了各种操作在 Guisu 中的数据流，展示数据如何在系统组件间流动。

### 目录

- [Apply 命令流程](#apply-命令流程)
- [Init 命令流程](#init-命令流程)
- [Add 命令流程](#add-命令流程)
- [Update 命令流程](#update-命令流程)
- [Edit 命令流程](#edit-命令流程)
- [模板渲染流程](#模板渲染流程)
- [加密解密流程](#加密解密流程)

---

### Apply 命令流程

`guisu apply` 命令是核心操作，将所有变更从源应用到目的地。

**关键决策点**：

1. **文件状态**（步骤 11）：
   - **已同步**：无需操作
   - **已添加**：新文件，创建它
   - **已修改**：文件在源中更改，更新它
   - **冲突**：源和目的地都更改了

2. **交互模式**（步骤 12）：
   - **是**：显示 TUI，让用户决定
   - **否**：自动应用（覆盖目的地）

3. **用户操作**（步骤 12）：
   - **覆盖**：用目标替换目的地
   - **跳过**：保持目的地不变，不更新数据库
   - **退出**：取消整个操作

**并行执行**：

步骤 5 和 6 使用 rayon 进行并行处理：
- 步骤 5：并行读取所有源文件
- 步骤 6：并行处理所有文件（解密 + 渲染）
- 步骤 8-14：顺序执行（为安全起见一次一个文件）

---

### Init 命令流程

`guisu init` 命令初始化新的 dotfiles 仓库。

**选项**：

```bash
guisu init owner/repo                    # GitHub 简写
guisu init https://github.com/user/repo  # 完整 URL
guisu init --ssh owner/repo              # 使用 SSH
guisu init --depth 1 owner/repo          # 浅克隆
guisu init --no-apply owner/repo         # 克隆后不应用
```

---

### Add 命令流程

`guisu add` 命令将文件从目的地添加到源。

**示例转换**：

```
输入                      源文件                    目标
~/.bashrc           →      .bashrc                 →  ~/.bashrc
~/.ssh/config       →      .ssh/config             →  ~/.ssh/config
~/.ssh/id_rsa       →      .ssh/id_rsa.age         →  ~/.ssh/id_rsa（加密）
~/.config/nvim/init.vim →  .config/nvim/init.vim   →  ~/.config/nvim/init.vim
```

**选项**：

```bash
guisu add ~/.bashrc                  # 简单添加
guisu add --encrypt ~/.ssh/id_rsa    # 加密添加
guisu add --template ~/.gitconfig    # 添加为模板（.j2）
guisu add ~/.config/nvim             # 递归添加目录
```

---

### Update 命令流程

`guisu update` 命令拉取最新变更并应用它们。

**选项**：

```bash
guisu update                # 获取 + 合并 + 应用
guisu update --no-apply     # 仅获取 + 合并
guisu update --rebase       # 使用 rebase 而非 merge
```

---

### Edit 命令流程

`guisu edit` 命令编辑源文件，透明处理解密/加密。

**安全考虑**：

1. **临时文件**：解密的内容写入临时文件
   - 应该在安全位置（/tmp，权限 0600）
   - 编辑后删除
   - 未来：安全擦除（删除前覆盖）

2. **编辑器安全**：编辑器可能创建备份
   - 警告用户关于编辑器备份文件
   - 记录如何禁用（例如，nvim：`set nobackup noswapfile`）

---

### 模板渲染流程

**示例模板**：

```jinja2
# ~/.bashrc - 在 {{ os }}（{{ arch }}）上渲染

# 平台特定
{% if os == "darwin" %}
export HOMEBREW_PREFIX="/opt/homebrew"
alias ls="gls --color=auto"
{% elif os == "linux" %}
alias ls="ls --color=auto"
{% endif %}

# 用户配置
export EDITOR="{{ editor }}"
export EMAIL="{{ email }}"

# 密钥
export GITHUB_TOKEN="{{ bitwarden("GitHub").login.password }}"
```

**上下文数据**：

```json
{
  "os": "darwin",
  "arch": "aarch64",
  "hostname": "macbook",
  "username": "user",
  "editor": "nvim",
  "email": "user@example.com",
  "config": {
    "age": { "symmetric": true },
    "bitwarden": { "provider": "rbw" }
  }
}
```

---

### 加密解密流程

**内联加密/解密**：

```
模板：
export TOKEN="{{ 'age:base64,YWdl...' | decrypt }}"

流程：
1. 解析模板
2. 调用 decrypt 过滤器
3. 内联解密：age:base64,... → 明文
4. 替换到模板中
5. 结果：export TOKEN="ghp_xxxxxxxxxxxx"
```
