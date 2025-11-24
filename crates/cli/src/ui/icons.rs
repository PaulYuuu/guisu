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
    /// Generic file icon
    pub const FILE: &'static str = "\u{f15b}";
    /// Directory/folder icon
    pub const DIRECTORY: &'static str = "\u{e5ff}";
    /// Symbolic link icon
    pub const SYMLINK: &'static str = "\u{f0c1}";

    // Config files
    /// Generic configuration file icon
    pub const CONFIG: &'static str = "\u{f107b}";
    /// JSON file icon
    pub const JSON: &'static str = "\u{e60b}";
    /// YAML file icon
    pub const YAML: &'static str = "\u{e6a8}";
    /// TOML file icon
    pub const TOML: &'static str = "\u{e6b2}";
    /// INI file icon
    pub const INI: &'static str = "\u{e652}";
    /// Environment file icon
    pub const ENV: &'static str = "\u{f462}";

    // Shell scripts
    /// Generic shell script icon
    pub const SHELL: &'static str = "\u{f1183}";
    /// Bash script icon
    pub const BASH: &'static str = "\u{f1183}";
    /// Zsh script icon
    pub const ZSH: &'static str = "\u{f1183}";
    /// Fish script icon
    pub const FISH: &'static str = "\u{f1183}";
    /// Nushell script icon
    pub const NUSHELL: &'static str = "\u{f1183}";

    // Text/Documentation
    /// Text file icon
    pub const TEXT: &'static str = "\u{f15c}";
    /// Markdown file icon
    pub const MARKDOWN: &'static str = "\u{f48a}";
    /// README file icon
    pub const README: &'static str = "\u{f00ba}";

    // Programming languages
    /// Rust source file icon
    pub const RUST: &'static str = "\u{e68b}";
    /// Python source file icon
    pub const PYTHON: &'static str = "\u{e606}";
    /// JavaScript source file icon
    pub const JAVASCRIPT: &'static str = "\u{e74e}";
    /// TypeScript source file icon
    pub const TYPESCRIPT: &'static str = "\u{e628}";
    /// Java source file icon
    pub const JAVA: &'static str = "\u{e256}";
    /// Go source file icon
    pub const GO: &'static str = "\u{e65e}";
    /// C source file icon
    pub const C: &'static str = "\u{e61e}";
    /// C++ source file icon
    pub const CPP: &'static str = "\u{e61d}";
    /// Ruby source file icon
    pub const RUBY: &'static str = "\u{e739}";
    /// PHP source file icon
    pub const PHP: &'static str = "\u{e73d}";
    /// HTML file icon
    pub const HTML: &'static str = "\u{f13b}";
    /// CSS file icon
    pub const CSS: &'static str = "\u{e749}";

    // Version control
    /// Git repository icon
    pub const GIT: &'static str = "\u{f02a2}";

    // Package managers
    /// NPM package icon
    pub const NPM: &'static str = "\u{e71e}";
    /// Cargo package icon
    pub const CARGO: &'static str = "\u{e68b}";

    // Other
    /// Nix file icon
    pub const NIX: &'static str = "\u{f313}";
    /// Docker file icon
    pub const DOCKER: &'static str = "\u{e650}";
    /// Database file icon
    pub const DATABASE: &'static str = "\u{f1c0}";
    /// Image file icon
    pub const IMAGE: &'static str = "\u{f1c5}";
    /// Video file icon
    pub const VIDEO: &'static str = "\u{f03d}";
    /// Audio file icon
    pub const AUDIO: &'static str = "\u{f001}";
    /// Archive file icon
    pub const ARCHIVE: &'static str = "\u{f410}";
    /// PDF file icon
    pub const PDF: &'static str = "\u{f1c1}";

    // Status icons (simple text)
    /// Success status icon [OK]
    pub const STATUS_SUCCESS: &'static str = "[OK]";
    /// Warning status icon [!]
    pub const STATUS_WARNING: &'static str = "[!]";
    /// Error status icon [X]
    pub const STATUS_ERROR: &'static str = "[X]";
    /// Info status icon [i]
    pub const STATUS_INFO: &'static str = "[i]";
    /// Hook status icon [*]
    pub const STATUS_HOOK: &'static str = "[*]";
    /// Running status icon [>]
    pub const STATUS_RUNNING: &'static str = "[>]";

    // Action icons for diff UI (simple ASCII markers)
    /// Add action marker +
    pub const ACTION_ADD: &'static str = "+";
    /// Modify action marker ~
    pub const ACTION_MODIFY: &'static str = "~";
    /// Remove action marker -
    pub const ACTION_REMOVE: &'static str = "-";
}

/// Status icon type
#[derive(Debug, Clone, Copy)]
pub enum StatusIcon {
    /// Success status ([OK])
    Success,
    /// Warning status ([!])
    Warning,
    /// Error status ([X])
    Error,
    /// Information status ([i])
    Info,
    /// Hook execution status ([*])
    Hook,
    /// Running status ([>])
    Running,
}

impl StatusIcon {
    /// Get icon based on `nerd_fonts` setting
    /// Note: Currently returns same text representation regardless of `nerd_fonts` setting
    #[must_use]
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
    /// File path for filename and extension detection
    pub path: &'a str,
    /// Whether the file is a directory
    pub is_directory: bool,
    /// Whether the file is a symbolic link
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
/// When `use_nerd_fonts` is false, returns empty string (no icon display).
/// When `use_nerd_fonts` is true, returns Nerd Font icons.
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    // Tests for StatusIcon

    #[test]
    fn test_status_icon_success() {
        let icon = StatusIcon::Success;
        assert_eq!(icon.get(true), "[OK]");
        assert_eq!(icon.get(false), "[OK]");
    }

    #[test]
    fn test_status_icon_warning() {
        let icon = StatusIcon::Warning;
        assert_eq!(icon.get(true), "[!]");
        assert_eq!(icon.get(false), "[!]");
    }

    #[test]
    fn test_status_icon_error() {
        let icon = StatusIcon::Error;
        assert_eq!(icon.get(true), "[X]");
        assert_eq!(icon.get(false), "[X]");
    }

    #[test]
    fn test_status_icon_info() {
        let icon = StatusIcon::Info;
        assert_eq!(icon.get(true), "[i]");
        assert_eq!(icon.get(false), "[i]");
    }

    #[test]
    fn test_status_icon_hook() {
        let icon = StatusIcon::Hook;
        assert_eq!(icon.get(true), "[*]");
        assert_eq!(icon.get(false), "[*]");
    }

    #[test]
    fn test_status_icon_running() {
        let icon = StatusIcon::Running;
        assert_eq!(icon.get(true), "[>]");
        assert_eq!(icon.get(false), "[>]");
    }

    // Tests for icon_for_file

    #[test]
    fn test_icon_for_file_no_nerd_fonts() {
        let info = FileIconInfo {
            path: "test.rs",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, false), "");
    }

    #[test]
    fn test_icon_for_file_directory() {
        let info = FileIconInfo {
            path: "src",
            is_directory: true,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::DIRECTORY);
    }

    #[test]
    fn test_icon_for_file_symlink() {
        let info = FileIconInfo {
            path: "link",
            is_directory: false,
            is_symlink: true,
        };

        assert_eq!(icon_for_file(&info, true), Icons::SYMLINK);
    }

    #[test]
    fn test_icon_for_file_symlink_priority_over_directory() {
        // Symlink takes priority over directory
        let info = FileIconInfo {
            path: "link_to_dir",
            is_directory: true,
            is_symlink: true,
        };

        assert_eq!(icon_for_file(&info, true), Icons::SYMLINK);
    }

    #[test]
    fn test_icon_for_file_exact_filename_cargo_toml() {
        let info = FileIconInfo {
            path: "Cargo.toml",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::CARGO);
    }

    #[test]
    fn test_icon_for_file_exact_filename_package_json() {
        let info = FileIconInfo {
            path: "package.json",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::NPM);
    }

    #[test]
    fn test_icon_for_file_exact_filename_gitignore() {
        let info = FileIconInfo {
            path: ".gitignore",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::GIT);
    }

    #[test]
    fn test_icon_for_file_exact_filename_readme() {
        let info = FileIconInfo {
            path: "README.md",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::README);
    }

    #[test]
    fn test_icon_for_file_exact_filename_dockerfile() {
        let info = FileIconInfo {
            path: "Dockerfile",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::DOCKER);
    }

    #[test]
    fn test_icon_for_file_extension_rust() {
        let info = FileIconInfo {
            path: "main.rs",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::RUST);
    }

    #[test]
    fn test_icon_for_file_extension_python() {
        let info = FileIconInfo {
            path: "script.py",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::PYTHON);
    }

    #[test]
    fn test_icon_for_file_extension_javascript() {
        let info = FileIconInfo {
            path: "app.js",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::JAVASCRIPT);
    }

    #[test]
    fn test_icon_for_file_extension_typescript() {
        let info = FileIconInfo {
            path: "index.ts",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::TYPESCRIPT);
    }

    #[test]
    fn test_icon_for_file_extension_shell() {
        let info = FileIconInfo {
            path: "setup.sh",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::SHELL);
    }

    #[test]
    fn test_icon_for_file_extension_markdown() {
        let info = FileIconInfo {
            path: "notes.md",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::MARKDOWN);
    }

    #[test]
    fn test_icon_for_file_extension_yaml() {
        let info = FileIconInfo {
            path: "config.yaml",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::YAML);
    }

    #[test]
    fn test_icon_for_file_extension_toml() {
        let info = FileIconInfo {
            path: "settings.toml",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::TOML);
    }

    #[test]
    fn test_icon_for_file_extension_json() {
        let info = FileIconInfo {
            path: "data.json",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::JSON);
    }

    #[test]
    fn test_icon_for_file_extension_go() {
        let info = FileIconInfo {
            path: "main.go",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::GO);
    }

    #[test]
    fn test_icon_for_file_extension_c() {
        let info = FileIconInfo {
            path: "hello.c",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::C);
    }

    #[test]
    fn test_icon_for_file_extension_cpp() {
        let info = FileIconInfo {
            path: "hello.cpp",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::CPP);
    }

    #[test]
    fn test_icon_for_file_extension_nix() {
        let info = FileIconInfo {
            path: "shell.nix",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::NIX);
    }

    #[test]
    fn test_icon_for_file_extension_archive() {
        let info = FileIconInfo {
            path: "backup.tar.gz",
            is_directory: false,
            is_symlink: false,
        };

        // Should match .gz extension
        assert_eq!(icon_for_file(&info, true), Icons::ARCHIVE);
    }

    #[test]
    fn test_icon_for_file_extension_image() {
        let info = FileIconInfo {
            path: "photo.png",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::IMAGE);
    }

    #[test]
    fn test_icon_for_file_extension_pdf() {
        let info = FileIconInfo {
            path: "document.pdf",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::PDF);
    }

    #[test]
    fn test_icon_for_file_default() {
        let info = FileIconInfo {
            path: "unknown.xyz",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::FILE);
    }

    #[test]
    fn test_icon_for_file_no_extension() {
        let info = FileIconInfo {
            path: "Makefile",
            is_directory: false,
            is_symlink: false,
        };

        // No exact match, no extension -> default file icon
        assert_eq!(icon_for_file(&info, true), Icons::FILE);
    }

    #[test]
    fn test_icon_for_file_with_path() {
        let info = FileIconInfo {
            path: "src/main.rs",
            is_directory: false,
            is_symlink: false,
        };

        // Should extract filename and match .rs extension
        assert_eq!(icon_for_file(&info, true), Icons::RUST);
    }

    #[test]
    fn test_icon_for_file_deep_path() {
        let info = FileIconInfo {
            path: "a/b/c/d/script.py",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::PYTHON);
    }

    #[test]
    fn test_icon_for_file_exact_match_priority() {
        // README.md should match exact filename (README.md) not extension (.md)
        let info = FileIconInfo {
            path: "README.md",
            is_directory: false,
            is_symlink: false,
        };

        assert_eq!(icon_for_file(&info, true), Icons::README);
    }

    #[test]
    fn test_icon_for_file_hidden_file_with_extension() {
        let info = FileIconInfo {
            path: ".bashrc",
            is_directory: false,
            is_symlink: false,
        };

        // Should match exact filename
        assert_eq!(icon_for_file(&info, true), Icons::BASH);
    }

    #[test]
    fn test_icon_constants_not_empty() {
        // Verify icon constants are defined (not empty)
        assert!(!Icons::FILE.is_empty());
        assert!(!Icons::DIRECTORY.is_empty());
        assert!(!Icons::SYMLINK.is_empty());
        assert!(!Icons::RUST.is_empty());
        assert!(!Icons::PYTHON.is_empty());
    }

    #[test]
    fn test_status_icon_clone() {
        let icon = StatusIcon::Success;
        let cloned = icon;

        assert_eq!(icon.get(true), cloned.get(true));
    }

    #[test]
    fn test_filename_icons_initialized() {
        // Verify the lazy static is properly initialized
        assert!(FILENAME_ICONS.contains_key("Cargo.toml"));
        assert!(FILENAME_ICONS.contains_key("package.json"));
        assert!(FILENAME_ICONS.contains_key(".gitignore"));
    }

    #[test]
    fn test_extension_icons_initialized() {
        // Verify the lazy static is properly initialized
        assert!(EXTENSION_ICONS.contains_key("rs"));
        assert!(EXTENSION_ICONS.contains_key("py"));
        assert!(EXTENSION_ICONS.contains_key("js"));
    }
}
