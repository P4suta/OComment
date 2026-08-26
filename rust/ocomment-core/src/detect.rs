use crate::{Dialect, Language};
use std::path::Path;

/// What [`detect_language`] concluded about a file, and on what evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Detection {
    /// The language to scan the file as.
    pub language: Language,
    /// The dialect that goes with it, [`Dialect::Standard`] unless the
    /// evidence named a more specific one.
    pub dialect: Dialect,
    /// What decided it: `extension`, `reserved-filename`, `shebang`, or
    /// `content`.
    pub reason: &'static str,
}

impl Detection {
    const fn new(language: Language, dialect: Dialect, reason: &'static str) -> Self {
        Self {
            language,
            dialect,
            reason,
        }
    }
}

/// Detect a built-in language from filename, shebang, then conservative content hints.
///
/// The evidence is weighed in that order and the first answer wins, so a
/// `.py` file whose first line says `#!/bin/sh` is still Python. `path` is
/// optional because a buffer in an editor may have no name yet; with no path
/// and no shebang, only a handful of unmistakable content hints are left, and
/// `None` means the caller has to name the language itself.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use ocomment_core::{Dialect, Language, detect_language};
///
/// let found = detect_language(Some(Path::new("src/app.tsx")), b"").unwrap();
/// assert_eq!(found.language, Language::TypeScript);
/// assert_eq!(found.dialect, Dialect::Tsx);
/// assert_eq!(found.reason, "extension");
///
/// // No name, so the shebang decides.
/// let piped = detect_language(None, b"#!/usr/bin/env python3\n").unwrap();
/// assert_eq!(piped.language, Language::Python);
///
/// // Nothing to go on.
/// assert!(detect_language(None, b"x = 1\n").is_none());
/// ```
pub fn detect_language(path: Option<&Path>, source: &[u8]) -> Option<Detection> {
    if let Some(path) = path {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let lower = name.to_ascii_lowercase();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let by_extension = match extension.as_str() {
            "rs" => Some((Language::Rust, Dialect::Standard)),
            "ml" | "mli" | "mlt" => Some((Language::Ocaml, Dialect::Standard)),
            "c" | "h" => Some((Language::C, Dialect::Standard)),
            "m" => Some((Language::C, Dialect::ObjectiveC)),
            "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => Some((Language::Cpp, Dialect::Standard)),
            "mm" => Some((Language::Cpp, Dialect::ObjectiveCpp)),
            "cu" | "cuh" => Some((Language::Cpp, Dialect::Cuda)),
            "go" => Some((Language::Go, Dialect::Standard)),
            "java" => Some((Language::Java, Dialect::Standard)),
            "js" | "mjs" | "cjs" => Some((Language::JavaScript, Dialect::Standard)),
            "jsx" => Some((Language::JavaScript, Dialect::Jsx)),
            "ts" | "mts" | "cts" => Some((Language::TypeScript, Dialect::Standard)),
            "tsx" => Some((Language::TypeScript, Dialect::Tsx)),
            "py" | "pyw" | "pyi" => Some((Language::Python, Dialect::Standard)),
            "sh" => Some((Language::Shell, Dialect::PosixSh)),
            "bash" => Some((Language::Shell, Dialect::Bash53)),
            "zsh" => Some((Language::Shell, Dialect::Zsh)),
            "html" | "htm" | "xhtml" | "shtml" => Some((Language::Html, Dialect::Standard)),
            "css" => Some((Language::Css, Dialect::Standard)),
            "jsonc" | "json5" => Some((Language::Jsonc, Dialect::Standard)),
            "sql" => Some((Language::Sql, Dialect::Standard)),
            "kt" | "kts" => Some((Language::Kotlin, Dialect::Standard)),
            _ => None,
        };
        if let Some((language, dialect)) = by_extension {
            return Some(Detection::new(language, dialect, "extension"));
        }
        let reserved = match lower.as_str() {
            "dockerfile" | "containerfile" | ".profile" | ".bashrc" | ".zshrc" => {
                Some((Language::Shell, Dialect::PosixSh))
            }
            "makefile" | "gnumakefile" => Some((Language::Shell, Dialect::PosixSh)),
            "tsconfig.json" | "jsconfig.json" => Some((Language::Jsonc, Dialect::Standard)),
            _ => None,
        };
        if let Some((language, dialect)) = reserved {
            return Some(Detection::new(language, dialect, "reserved-filename"));
        }
    }

    let first_line = source.split(|byte| *byte == b'\n').next().unwrap_or(source);
    if first_line.starts_with(b"#!") {
        let line = String::from_utf8_lossy(first_line).to_ascii_lowercase();
        if line.contains("python") {
            return Some(Detection::new(
                Language::Python,
                Dialect::Standard,
                "shebang",
            ));
        }
        if line.contains("bash") {
            return Some(Detection::new(Language::Shell, Dialect::Bash53, "shebang"));
        }
        if line.contains("zsh") {
            return Some(Detection::new(Language::Shell, Dialect::Zsh, "shebang"));
        }
        if line.contains("sh") {
            return Some(Detection::new(Language::Shell, Dialect::PosixSh, "shebang"));
        }
        if line.contains("node") | line.contains("deno") {
            return Some(Detection::new(
                Language::JavaScript,
                Dialect::Standard,
                "shebang",
            ));
        }
    }

    let prefix = &source[..source.len().min(4096)];
    let text = String::from_utf8_lossy(prefix).to_ascii_lowercase();
    if text.contains("<!doctype html") || text.contains("<html") {
        return Some(Detection::new(Language::Html, Dialect::Standard, "content"));
    }
    if text.trim_start().starts_with("<?xml") && text.contains("<html") {
        return Some(Detection::new(Language::Html, Dialect::Standard, "content"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extensions_and_shebangs() {
        assert_eq!(
            detect_language(Some(Path::new("x.tsx")), b"")
                .unwrap()
                .dialect,
            Dialect::Tsx
        );
        assert_eq!(
            detect_language(None, b"#!/usr/bin/env python3\n")
                .unwrap()
                .language,
            Language::Python
        );
    }
}
