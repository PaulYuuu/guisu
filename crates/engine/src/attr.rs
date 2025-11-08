//! File attribute parsing and encoding
//!
//! This module handles the parsing of attributes from source filenames and
//! encoding them back into filenames.
//!
//! # Attribute Encoding
//!
//! Attributes are encoded using file extensions and permissions:
//!
//! - `.j2` - File is a Jinja2 template
//! - `.age` - File is encrypted with age
//! - `.j2.age` - Template that is encrypted (edit decrypts, render encrypts)
//! - File permissions (Unix):
//!   - `0600` / `0700` - Private files/directories
//!   - `0755` - Executable files
//!
//! Target filename is source filename with extensions removed:
//! - `.gitconfig.j2` → `~/.gitconfig`
//! - `secrets.age` → `~/secrets`
//! - `config.j2.age` → `~/config`
//!
//! # Examples
//!
//! ```
//! use guisu_engine::attr::FileAttributes;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Parse from source file (extensions + permissions)
//! let (attrs, target_name) = FileAttributes::parse_from_source(".gitconfig.j2", Some(0o644))?;
//! assert!(attrs.is_template());
//! assert_eq!(target_name, ".gitconfig");
//!
//! // Encrypted file with private permissions
//! let (attrs, target_name) = FileAttributes::parse_from_source("secrets.age", Some(0o600))?;
//! assert!(attrs.is_encrypted());
//! assert!(attrs.is_private());
//! assert_eq!(target_name, "secrets");
//! # Ok(())
//! # }
//! ```

use crate::error::Result;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Unix permission constants
const PERMISSION_MASK: u32 = 0o777;
const PRIVATE_FILE: u32 = 0o600;
const PRIVATE_DIR: u32 = 0o700;
const OWNER_EXECUTE: u32 = 0o100;
const ALL_WRITE: u32 = 0o222;
const READONLY: u32 = 0o444;
const READONLY_EXEC: u32 = 0o555;
const STANDARD_EXEC: u32 = 0o755;

bitflags::bitflags! {
    /// Attributes that can be encoded in a filename
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FileAttributes: u8 {
        /// Should this file be hidden (start with a dot)?
        const DOT = 1 << 0;
        /// Should this file have restrictive permissions (private)?
        const PRIVATE = 1 << 1;
        /// Should this file be read-only?
        const READONLY = 1 << 2;
        /// Should this file be executable?
        const EXECUTABLE = 1 << 3;
        /// Should this file be processed as a template?
        const TEMPLATE = 1 << 4;
        /// Is this file encrypted?
        const ENCRYPTED = 1 << 5;
    }
}

impl FileAttributes {
    /// Create attributes with all flags set to false
    pub fn new() -> Self {
        Self::empty()
    }

    /// Check if file should be hidden (start with a dot)
    #[inline]
    pub fn is_dot(&self) -> bool {
        self.contains(Self::DOT)
    }

    /// Check if file should have restrictive permissions (private)
    #[inline]
    pub fn is_private(&self) -> bool {
        self.contains(Self::PRIVATE)
    }

    /// Check if file should be read-only
    #[inline]
    pub fn is_readonly(&self) -> bool {
        self.contains(Self::READONLY)
    }

    /// Check if file should be executable
    #[inline]
    pub fn is_executable(&self) -> bool {
        self.contains(Self::EXECUTABLE)
    }

    /// Check if file should be processed as a template
    #[inline]
    pub fn is_template(&self) -> bool {
        self.contains(Self::TEMPLATE)
    }

    /// Check if file is encrypted
    #[inline]
    pub fn is_encrypted(&self) -> bool {
        self.contains(Self::ENCRYPTED)
    }

    /// Set whether file should be hidden (start with a dot)
    #[inline]
    pub fn set_dot(&mut self, value: bool) {
        self.set(Self::DOT, value);
    }

    /// Set whether file should have restrictive permissions (private)
    #[inline]
    pub fn set_private(&mut self, value: bool) {
        self.set(Self::PRIVATE, value);
    }

    /// Set whether file should be read-only
    #[inline]
    pub fn set_readonly(&mut self, value: bool) {
        self.set(Self::READONLY, value);
    }

    /// Set whether file should be executable
    #[inline]
    pub fn set_executable(&mut self, value: bool) {
        self.set(Self::EXECUTABLE, value);
    }

    /// Set whether file should be processed as a template
    #[inline]
    pub fn set_template(&mut self, value: bool) {
        self.set(Self::TEMPLATE, value);
    }

    /// Set whether file is encrypted
    #[inline]
    pub fn set_encrypted(&mut self, value: bool) {
        self.set(Self::ENCRYPTED, value);
    }

    /// Parse attributes from a source file
    ///
    /// Returns the parsed attributes and the target filename (with extensions stripped).
    ///
    /// # Arguments
    ///
    /// * `filename` - The source filename (e.g., `.gitconfig.j2`, `secrets.age`)
    /// * `mode` - Optional Unix file mode for permission detection
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_engine::attr::FileAttributes;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Template file
    /// let (attrs, name) = FileAttributes::parse_from_source(".gitconfig.j2", Some(0o644))?;
    /// assert!(attrs.is_template());
    /// assert_eq!(name, ".gitconfig");
    ///
    /// // Encrypted file with private permissions
    /// let (attrs, name) = FileAttributes::parse_from_source("secrets.age", Some(0o600))?;
    /// assert!(attrs.is_encrypted());
    /// assert!(attrs.is_private());
    /// assert_eq!(name, "secrets");
    ///
    /// // Executable script
    /// let (attrs, name) = FileAttributes::parse_from_source("deploy.sh", Some(0o755))?;
    /// assert!(attrs.is_executable());
    /// assert_eq!(name, "deploy.sh");
    /// # Ok(())
    /// # }
    /// ```
    pub fn parse_from_source(filename: &str, mode: Option<u32>) -> Result<(Self, String)> {
        let mut attrs = Self::new();
        let mut target_name = filename.to_string();

        // Check for .age extension (must be last)
        if target_name.ends_with(".age") {
            attrs.set_encrypted(true);
            target_name = target_name
                .strip_suffix(".age")
                .expect("checked with ends_with")
                .to_string();
        }

        // Check for .j2 extension (before .age)
        if target_name.ends_with(".j2") {
            attrs.set_template(true);
            target_name = target_name
                .strip_suffix(".j2")
                .expect("checked with ends_with")
                .to_string();
        }

        // Parse permissions from Unix mode
        if let Some(mode) = mode {
            attrs.parse_permissions(mode);
        }

        Ok((attrs, target_name))
    }

    /// Parse Unix permissions to set attributes
    ///
    /// Detects private, executable, and readonly attributes from file mode.
    fn parse_permissions(&mut self, mode: u32) {
        // Extract permission bits (last 9 bits)
        let perms = mode & PERMISSION_MASK;

        // Check for private files (0600 for files, 0700 for directories)
        // Private means owner-only read/write, no group or other permissions
        if perms == PRIVATE_FILE || perms == PRIVATE_DIR {
            self.set_private(true);
        }

        // Check for executable (owner execute bit set)
        if (perms & OWNER_EXECUTE) != 0 {
            self.set_executable(true);
        }

        // Check for readonly (no write bits set)
        if (perms & ALL_WRITE) == 0 {
            self.set_readonly(true);
        }
    }

    /// Get the Unix file permission mode for these attributes
    ///
    /// Returns `None` if no specific permissions are required (use defaults).
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_engine::attr::FileAttributes;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Private directory (0700)
    /// let (attrs, _) = FileAttributes::parse_from_source(".ssh", Some(0o700))?;
    /// assert_eq!(attrs.mode(), Some(0o700));
    ///
    /// // Executable script (0755)
    /// let (attrs, _) = FileAttributes::parse_from_source("script.sh", Some(0o755))?;
    /// assert_eq!(attrs.mode(), Some(0o755));
    /// # Ok(())
    /// # }
    /// ```
    pub fn mode(&self) -> Option<u32> {
        match (self.is_private(), self.is_readonly(), self.is_executable()) {
            (true, false, true) => Some(PRIVATE_DIR), // private + executable
            (true, false, false) => Some(PRIVATE_FILE), // private only
            (false, true, true) => Some(READONLY_EXEC), // readonly + executable
            (false, true, false) => Some(READONLY),   // readonly only
            (false, false, true) => Some(STANDARD_EXEC), // executable only
            (false, false, false) => None,            // use defaults
            _ => None,                                // invalid combination
        }
    }
}

// Custom Serialize to provide user-friendly JSON/TOML format
// Instead of serializing as a bitflags integer, we expose individual boolean fields
impl Serialize for FileAttributes {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("FileAttributes", 6)?;
        state.serialize_field("is_dot", &self.is_dot())?;
        state.serialize_field("is_private", &self.is_private())?;
        state.serialize_field("is_readonly", &self.is_readonly())?;
        state.serialize_field("is_executable", &self.is_executable())?;
        state.serialize_field("is_template", &self.is_template())?;
        state.serialize_field("is_encrypted", &self.is_encrypted())?;
        state.end()
    }
}

// Custom Deserialize to parse user-friendly JSON/TOML format
// Reads individual boolean fields and converts them to bitflags representation
impl<'de> Deserialize<'de> for FileAttributes {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        // Allow `Is` prefix for boolean attribute fields - it improves clarity
        // by explicitly indicating these are boolean flags (isDot, isPrivate, etc.)
        #[allow(clippy::enum_variant_names)]
        enum Field {
            IsDot,
            IsPrivate,
            IsReadonly,
            IsExecutable,
            IsTemplate,
            IsEncrypted,
        }

        struct FileAttributesVisitor;

        impl<'de> Visitor<'de> for FileAttributesVisitor {
            type Value = FileAttributes;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct FileAttributes")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<FileAttributes, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut attrs = FileAttributes::empty();

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::IsDot => {
                            let value: bool = map.next_value()?;
                            attrs.set(FileAttributes::DOT, value);
                        }
                        Field::IsPrivate => {
                            let value: bool = map.next_value()?;
                            attrs.set(FileAttributes::PRIVATE, value);
                        }
                        Field::IsReadonly => {
                            let value: bool = map.next_value()?;
                            attrs.set(FileAttributes::READONLY, value);
                        }
                        Field::IsExecutable => {
                            let value: bool = map.next_value()?;
                            attrs.set(FileAttributes::EXECUTABLE, value);
                        }
                        Field::IsTemplate => {
                            let value: bool = map.next_value()?;
                            attrs.set(FileAttributes::TEMPLATE, value);
                        }
                        Field::IsEncrypted => {
                            let value: bool = map.next_value()?;
                            attrs.set(FileAttributes::ENCRYPTED, value);
                        }
                    }
                }

                Ok(attrs)
            }
        }

        const FIELDS: &[&str] = &[
            "is_dot",
            "is_private",
            "is_readonly",
            "is_executable",
            "is_template",
            "is_encrypted",
        ];
        deserializer.deserialize_struct("FileAttributes", FIELDS, FileAttributesVisitor)
    }
}

impl Default for FileAttributes {
    fn default() -> Self {
        Self::new()
    }
}
