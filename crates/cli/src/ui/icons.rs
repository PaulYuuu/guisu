//! Icon system for file and status display
//!
//! Provides Nerd Font icons for different file types and status indicators.
//! Icons can be disabled via configuration to use simple text instead.

use indexmap::IndexMap;
use std::sync::LazyLock;

/// Icon constants using Nerd Font symbols
pub struct Icons;

impl Icons {
    // File types
    pub const FILE: &'static str = "\u{f15b}";
    pub const DIRECTORY: &'static str = "\u{e5ff}";
    pub const SYMLINK: &'static str = "\u{f0c1}";

    // Config files
    pub const CONFIG: &'static str = "\u{f107b}";
    pub const JSON: &'static str = "\u{e60b}";
    pub const YAML: &'static str = "\u{e6a8}";
    pub const TOML: &'static str = "\u{e6b2}";
    pub const INI: &'static str = "\u{e652}";
    pub const ENV: &'static str = "\u{f462}";

    // Shell scripts
    pub const SHELL: &'static str = "\u{f1183}";
    pub const BASH: &'static str = "\u{f1183}";
    pub const ZSH: &'static str = "\u{f1183}";
    pub const FISH: &'static str = "\u{f1183}";
    pub const NUSHELL: &'static str = "\u{f1183}";

    // Text/Documentation
    pub const TEXT: &'static str = "\u{f15c}";
    pub const MARKDOWN: &'static str = "\u{f48a}";
    pub const README: &'static str = "\u{f00ba}";

    // Programming languages
    pub const RUST: &'static str = "\u{e68b}";
    pub const PYTHON: &'static str = "\u{e606}";
    pub const JAVASCRIPT: &'static str = "\u{e74e}";
    pub const TYPESCRIPT: &'static str = "\u{e628}";
    pub const JAVA: &'static str = "\u{e256}";
    pub const GO: &'static str = "\u{e65e}";
    pub const C: &'static str = "\u{e61e}";
    pub const CPP: &'static str = "\u{e61d}";
    pub const RUBY: &'static str = "\u{e739}";
    pub const PHP: &'static str = "\u{e73d}";
    pub const HTML: &'static str = "\u{f13b}";
    pub const CSS: &'static str = "\u{e749}";

    // Version control
    pub const GIT: &'static str = "\u{f02a2}";

    // Package managers
    pub const NPM: &'static str = "\u{e71e}";
    pub const CARGO: &'static str = "\u{e68b}";

    // Other
    pub const NIX: &'static str = "\u{f313}";
    pub const DOCKER: &'static str = "\u{e650}";
    pub const DATABASE: &'static str = "\u{f1c0}";
    pub const IMAGE: &'static str = "\u{f1c5}";
    pub const VIDEO: &'static str = "\u{f03d}";
    pub const AUDIO: &'static str = "\u{f001}";
    pub const ARCHIVE: &'static str = "\u{f410}";
    pub const PDF: &'static str = "\u{f1c1}";

    // Status icons (simple text)
    pub const STATUS_SUCCESS: &'static str = "[OK]";
    pub const STATUS_WARNING: &'static str = "[!]";
    pub const STATUS_ERROR: &'static str = "[X]";
    pub const STATUS_INFO: &'static str = "[i]";
    pub const STATUS_HOOK: &'static str = "[*]";
    pub const STATUS_RUNNING: &'static str = "[>]";

    // Action icons for diff UI (simple ASCII markers)
    pub const ACTION_ADD: &'static str = "+";
    pub const ACTION_MODIFY: &'static str = "~";
    pub const ACTION_REMOVE: &'static str = "-";
}

/// Status icon type
#[derive(Debug, Clone, Copy)]
pub enum StatusIcon {
    Success,
    Warning,
    Error,
    Info,
    Hook,
    Running,
}

impl StatusIcon {
    /// Get icon based on nerd_fonts setting
    /// Note: Currently returns same text representation regardless of nerd_fonts setting
    pub fn get(&self, _use_nerd_fonts: bool) -> &'static str {
        match self {
            Self::Success => Icons::STATUS_SUCCESS,
            Self::Warning => Icons::STATUS_WARNING,
            Self::Error => Icons::STATUS_ERROR,
            Self::Info => Icons::STATUS_INFO,
            Self::Hook => Icons::STATUS_HOOK,
            Self::Running => Icons::STATUS_RUNNING,
        }
    }
}

/// Exact filename to icon mapping
static FILENAME_ICONS: LazyLock<IndexMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = IndexMap::new();

    // Special files
    m.insert(".gitignore", Icons::GIT);
    m.insert(".gitconfig", Icons::GIT);
    m.insert(".gitattributes", Icons::GIT);
    m.insert(".gitmodules", Icons::GIT);

    // Config files
    m.insert("Cargo.toml", Icons::CARGO);
    m.insert("Cargo.lock", Icons::CARGO);
    m.insert("package.json", Icons::NPM);
    m.insert("package-lock.json", Icons::NPM);
    m.insert("yarn.lock", Icons::NPM);
    m.insert("pnpm-lock.yaml", Icons::NPM);

    // Docker
    m.insert("Dockerfile", Icons::DOCKER);
    m.insert("docker-compose.yml", Icons::DOCKER);
    m.insert("docker-compose.yaml", Icons::DOCKER);
    m.insert(".dockerignore", Icons::DOCKER);

    // Documentation
    m.insert("README.md", Icons::README);
    m.insert("README.txt", Icons::README);
    m.insert("README", Icons::README);
    m.insert("LICENSE", Icons::TEXT);
    m.insert("LICENSE.md", Icons::TEXT);
    m.insert("CHANGELOG.md", Icons::MARKDOWN);

    // Environment
    m.insert(".env", Icons::ENV);
    m.insert(".env.local", Icons::ENV);
    m.insert(".env.example", Icons::ENV);

    // Shell configs
    m.insert(".bashrc", Icons::BASH);
    m.insert(".bash_profile", Icons::BASH);
    m.insert(".zshrc", Icons::ZSH);
    m.insert(".zshenv", Icons::ZSH);
    m.insert(".zprofile", Icons::ZSH);
    m.insert("config.fish", Icons::FISH);

    m
});

/// File extension to icon mapping
static EXTENSION_ICONS: LazyLock<IndexMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = IndexMap::new();

    // Config formats
    m.insert("toml", Icons::TOML);
    m.insert("yaml", Icons::YAML);
    m.insert("yml", Icons::YAML);
    m.insert("json", Icons::JSON);
    m.insert("ini", Icons::INI);
    m.insert("conf", Icons::CONFIG);
    m.insert("cfg", Icons::CONFIG);

    // Shell scripts
    m.insert("sh", Icons::SHELL);
    m.insert("bash", Icons::BASH);
    m.insert("zsh", Icons::ZSH);
    m.insert("fish", Icons::FISH);
    m.insert("nu", Icons::NUSHELL);

    // Text/docs
    m.insert("txt", Icons::TEXT);
    m.insert("md", Icons::MARKDOWN);
    m.insert("markdown", Icons::MARKDOWN);
    m.insert("rst", Icons::TEXT);
    m.insert("asciidoc", Icons::TEXT);
    m.insert("adoc", Icons::TEXT);

    // Programming languages
    m.insert("rs", Icons::RUST);
    m.insert("py", Icons::PYTHON);
    m.insert("js", Icons::JAVASCRIPT);
    m.insert("mjs", Icons::JAVASCRIPT);
    m.insert("cjs", Icons::JAVASCRIPT);
    m.insert("ts", Icons::TYPESCRIPT);
    m.insert("tsx", Icons::TYPESCRIPT);
    m.insert("jsx", Icons::JAVASCRIPT);
    m.insert("java", Icons::JAVA);
    m.insert("go", Icons::GO);
    m.insert("c", Icons::C);
    m.insert("h", Icons::C);
    m.insert("cpp", Icons::CPP);
    m.insert("cc", Icons::CPP);
    m.insert("cxx", Icons::CPP);
    m.insert("hpp", Icons::CPP);
    m.insert("rb", Icons::RUBY);
    m.insert("php", Icons::PHP);
    m.insert("html", Icons::HTML);
    m.insert("htm", Icons::HTML);
    m.insert("css", Icons::CSS);
    m.insert("scss", Icons::CSS);
    m.insert("sass", Icons::CSS);

    // Other
    m.insert("nix", Icons::NIX);
    m.insert("sql", Icons::DATABASE);
    m.insert("db", Icons::DATABASE);
    m.insert("sqlite", Icons::DATABASE);

    // Archives
    m.insert("zip", Icons::ARCHIVE);
    m.insert("tar", Icons::ARCHIVE);
    m.insert("gz", Icons::ARCHIVE);
    m.insert("bz2", Icons::ARCHIVE);
    m.insert("xz", Icons::ARCHIVE);
    m.insert("7z", Icons::ARCHIVE);
    m.insert("rar", Icons::ARCHIVE);

    // Images
    m.insert("png", Icons::IMAGE);
    m.insert("jpg", Icons::IMAGE);
    m.insert("jpeg", Icons::IMAGE);
    m.insert("gif", Icons::IMAGE);
    m.insert("svg", Icons::IMAGE);
    m.insert("webp", Icons::IMAGE);
    m.insert("ico", Icons::IMAGE);

    // Video
    m.insert("mp4", Icons::VIDEO);
    m.insert("mkv", Icons::VIDEO);
    m.insert("avi", Icons::VIDEO);
    m.insert("mov", Icons::VIDEO);
    m.insert("webm", Icons::VIDEO);

    // Audio
    m.insert("mp3", Icons::AUDIO);
    m.insert("flac", Icons::AUDIO);
    m.insert("wav", Icons::AUDIO);
    m.insert("ogg", Icons::AUDIO);
    m.insert("m4a", Icons::AUDIO);

    // Documents
    m.insert("pdf", Icons::PDF);

    m
});

/// File information for icon selection
pub struct FileIconInfo<'a> {
    pub path: &'a str,
    pub is_directory: bool,
    pub is_symlink: bool,
}

/// Get icon for a file based on its properties
///
/// Selection priority:
/// 1. File type attributes (symlink, directory)
/// 2. Exact filename match
/// 3. File extension match
/// 4. Special attributes (encrypted, executable, template)
/// 5. Default file icon
///
/// Note: File type (based on name/extension) takes priority over attributes like
/// encrypted/executable/template. This ensures that a .nu file shows the nushell icon
/// regardless of whether it's encrypted, and a .sh file shows the shell icon regardless
/// of whether it's executable.
///
/// When use_nerd_fonts is false, returns empty string (no icon display).
/// When use_nerd_fonts is true, returns Nerd Font icons.
pub fn icon_for_file(info: &FileIconInfo, use_nerd_fonts: bool) -> &'static str {
    // If not using Nerd Fonts, don't show any icons
    if !use_nerd_fonts {
        return "";
    }

    // Check file type first
    if info.is_symlink {
        return Icons::SYMLINK;
    }

    if info.is_directory {
        return Icons::DIRECTORY;
    }

    // Extract filename from path
    let filename = info.path.split('/').next_back().unwrap_or(info.path);

    // Try exact filename match
    if let Some(&icon) = FILENAME_ICONS.get(filename) {
        return icon;
    }

    // Try extension match - prioritize file type over attributes
    if let Some(ext) = filename.split('.').next_back()
        && ext != filename
    {
        // Make sure we actually have an extension
        if let Some(&icon) = EXTENSION_ICONS.get(ext) {
            return icon;
        }
    }

    // No file type match found, return default file icon
    // Note: Attributes (encrypted, executable, template) are not displayed as icons
    // They are tracked in the FileInfo.attributes field if needed for other purposes
    Icons::FILE
}
