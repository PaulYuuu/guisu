//! Integration tests for ignore pattern matching

use guisu_config::IgnoreMatcher;
use std::path::Path;

#[test]
fn test_ignore_patterns_with_negation() {
    let test_dir = std::env::temp_dir().join("guisu_ignore_test");
    std::fs::create_dir_all(test_dir.join(".guisu")).unwrap();

    // Create test ignore configuration
    std::fs::write(
        test_dir.join(".guisu/ignores.toml"),
        r#"
global = [
    # Simple patterns
    ".DS_Store",
    "*.log",
    "*.tmp",

    # Directory pattern
    "node_modules/",

    # Glob patterns
    "*.backup",
    "test-*.txt",

    # Complex case: ignore all .config, then re-include specific dirs
    ".config/*",
    "!.config/atuin/",
    "!.config/bat/",
    "!.config/nvim/",
    "!.config/zsh/",

    # Re-ignore specific files in included directories
    ".config/nvim/lazy-lock.json",
    ".config/zsh/conf.d/98_bitwarden.zsh",
    ".config/zsh/conf.d/99_private.zsh",
]

darwin = [
    ".Trash/",
    "Library/Caches/",
]
"#,
    )
    .unwrap();

    let matcher = IgnoreMatcher::from_ignores_toml(&test_dir).unwrap();

    // Test cases: (path, expected_ignored, description)
    let test_cases = vec![
        // 1. Simple patterns
        (".DS_Store", true, "Simple exact match"),
        ("debug.log", true, "Glob pattern *.log"),
        ("data.tmp", true, "Glob pattern *.tmp"),
        ("regular.txt", false, "No matching pattern"),
        // 2. Directory patterns
        (
            "node_modules/package.json",
            true,
            "Inside ignored directory",
        ),
        ("node_modules/", true, "Directory itself"),
        // 3. Glob patterns
        ("file.backup", true, "*.backup pattern"),
        ("test-data.txt", true, "test-*.txt pattern"),
        ("test.txt", false, "Doesn't match test-*.txt"),
        // 4. Complex .config case - excluded items should NOT be ignored
        (".config/random/file", true, "Matches .config/* - ignored"),
        (
            ".config/atuin/config.toml",
            false,
            "Matches !.config/atuin/ - NOT ignored",
        ),
        (
            ".config/bat/config",
            false,
            "Matches !.config/bat/ - NOT ignored",
        ),
        (
            ".config/nvim/init.lua",
            false,
            "Matches !.config/nvim/ - NOT ignored",
        ),
        (
            ".config/zsh/zshrc",
            false,
            "Matches !.config/zsh/ - NOT ignored",
        ),
        // 5. Re-ignore specific files in included directories
        (
            ".config/nvim/lazy-lock.json",
            true,
            "Re-ignored in included dir",
        ),
        (
            ".config/zsh/conf.d/98_bitwarden.zsh",
            true,
            "Re-ignored private file",
        ),
        (
            ".config/zsh/conf.d/99_private.zsh",
            true,
            "Re-ignored private file",
        ),
        (".config/zsh/conf.d/aliases.zsh", false, "Not re-ignored"),
    ];

    for (path, expected_ignored, description) in test_cases {
        let is_ignored = matcher.is_ignored(Path::new(path));
        assert_eq!(
            is_ignored, expected_ignored,
            "Failed: {} | path={} | ignored={} (expected={})",
            description, path, is_ignored, expected_ignored
        );
    }

    // Cleanup
    std::fs::remove_dir_all(&test_dir).ok();
}

#[test]
fn test_directory_name_without_trailing_slash() {
    let test_dir = std::env::temp_dir().join("guisu_ignore_test_dirnames");
    std::fs::create_dir_all(test_dir.join(".guisu")).unwrap();

    // Test directory names without trailing slashes
    std::fs::write(
        test_dir.join(".guisu/ignores.toml"),
        r#"
global = [
    # Directory names without trailing /
    "DankMaterialShell",
    "node_modules",
    ".git",
]
"#,
    )
    .unwrap();

    let matcher = IgnoreMatcher::from_ignores_toml(&test_dir).unwrap();

    // Test cases: directory name should match anywhere in path
    let test_cases = vec![
        // Direct directory match
        ("DankMaterialShell", true, "Directory name at root"),
        ("DankMaterialShell/config", true, "File inside directory"),
        (
            ".config/DankMaterialShell",
            true,
            "Directory in subdirectory",
        ),
        (
            ".config/DankMaterialShell/theme.json",
            true,
            "File in nested directory",
        ),
        // node_modules
        ("node_modules", true, "node_modules at root"),
        ("node_modules/package.json", true, "File in node_modules"),
        ("project/node_modules", true, "node_modules nested"),
        (
            "project/node_modules/lib/file.js",
            true,
            "File in nested node_modules",
        ),
        // .git
        (".git", true, ".git directory"),
        (".git/config", true, "File in .git"),
        ("repo/.git/HEAD", true, ".git nested"),
        // Should NOT match
        (
            "DankMaterialShell.txt",
            false,
            "Filename contains pattern but not a directory",
        ),
        (
            ".config/MyDankMaterialShell",
            false,
            "Pattern is part of name",
        ),
        ("node_modules_backup", false, "Pattern is part of name"),
    ];

    for (path, expected_ignored, description) in test_cases {
        let is_ignored = matcher.is_ignored(Path::new(path));
        assert_eq!(
            is_ignored, expected_ignored,
            "Failed: {} | path={} | ignored={} (expected={})",
            description, path, is_ignored, expected_ignored
        );
    }

    std::fs::remove_dir_all(&test_dir).ok();
}

#[test]
#[cfg(target_os = "macos")]
fn test_platform_specific_patterns_macos() {
    let test_dir = std::env::temp_dir().join("guisu_ignore_test_macos");
    std::fs::create_dir_all(test_dir.join(".guisu")).unwrap();

    std::fs::write(
        test_dir.join(".guisu/ignores.toml"),
        r#"
darwin = [
    ".Trash/",
    "Library/Caches/",
]
"#,
    )
    .unwrap();

    let matcher = IgnoreMatcher::from_ignores_toml(&test_dir).unwrap();

    assert!(matcher.is_ignored(Path::new(".Trash/file")));
    assert!(matcher.is_ignored(Path::new("Library/Caches/data")));
    assert!(!matcher.is_ignored(Path::new("Library/Application Support/file")));

    std::fs::remove_dir_all(&test_dir).ok();
}
