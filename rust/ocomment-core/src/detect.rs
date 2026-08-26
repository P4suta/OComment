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

/// The interpreter names a `#!` line is read for, in the order they are tried,
/// with the language and dialect each one selects.
///
/// The line is searched for each name as a substring rather than split into
/// words, because an interpreter arrives written a dozen ways: as a path
/// (`#!/bin/bash`), with a version (`#!/usr/bin/python3.12`), or behind `env`
/// with options (`#!/usr/bin/env -S node --enable-source-maps`). The order is
/// therefore part of the rule and not an accident of listing: `bash` and `zsh`
/// both *contain* `sh`, so each has to be met before it, or every Bash script
/// on disk would be read as POSIX shell. `luajit` contains `lua`, and `jruby`
/// and `truffleruby` contain `ruby`, and all three are listed before the name
/// they contain under the same convention, though those pairs name the same
/// language whichever of the two is met first.
const SHEBANGS: [(&str, Language, Dialect); 12] = [
    ("python", Language::Python, Dialect::Standard),
    ("bash", Language::Shell, Dialect::Bash53),
    ("zsh", Language::Shell, Dialect::Zsh),
    ("luajit", Language::Lua, Dialect::Standard),
    ("lua", Language::Lua, Dialect::Standard),
    ("php", Language::Php, Dialect::Standard),
    ("truffleruby", Language::Ruby, Dialect::Standard),
    ("jruby", Language::Ruby, Dialect::Standard),
    ("ruby", Language::Ruby, Dialect::Standard),
    ("sh", Language::Shell, Dialect::PosixSh),
    ("node", Language::JavaScript, Dialect::Standard),
    ("deno", Language::JavaScript, Dialect::Standard),
];

/// Every interpreter name [`detect_language`] reads a `#!` line for, in the
/// order it tries them.
///
/// This is the detector's own table rather than a copy of it, so a caller that
/// documents or publishes the list — `spec/languages.toml` does, and
/// `ocomment languages` prints it — can be checked against what the detector
/// will actually answer to instead of against a second list that may have
/// stopped agreeing.
///
/// A name matches anywhere in the first line, so these are the substrings to
/// look for and not the whole words to compare against. The order they come
/// back in is the order they must be tried in: `bash` and `zsh` both contain
/// `sh`, so each has to be met before it, or every Bash script on disk would
/// be read as POSIX shell.
///
/// # Examples
///
/// ```
/// use ocomment_core::{Language, detect_language, shebang_interpreters};
///
/// // Every published name really does select a language from a `#!` line.
/// for interpreter in shebang_interpreters() {
///     let line = format!("#!/usr/bin/env {interpreter}\n");
///     let found = detect_language(None, line.as_bytes()).unwrap();
///     assert_eq!(found.reason, "shebang");
/// }
/// // `bash` is met before `sh`, which it contains.
/// let bash = detect_language(None, b"#!/bin/bash\n").unwrap();
/// assert_eq!(bash.dialect, ocomment_core::Dialect::Bash53);
/// ```
pub fn shebang_interpreters() -> impl Iterator<Item = &'static str> {
    SHEBANGS.iter().map(|(name, _, _)| *name)
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
            "toml" => Some((Language::Toml, Dialect::Standard)),
            "lua" | "rockspec" => Some((Language::Lua, Dialect::Standard)),
            "yml" | "yaml" => Some((Language::Yaml, Dialect::Standard)),
            /* NOTE: `.php5` and `.inc` are deliberately absent: the first is a
             * migration-era suffix no supported PHP version installs a handler
             * for, and the second names a file included by another language
             * quite as often as by PHP. */
            "php" | "phtml" | "phpt" => Some((Language::Php, Dialect::Standard)),
            /* NOTE: Ruby owns more suffixes than any other language here, because
             * a Ruby project writes so much of itself in Ruby: `.rake` for a
             * Rake task file, `.gemspec` for a gem's own manifest, `.ru` for a
             * Rack configuration, `.podspec` and `.jbuilder` and `.thor` for
             * three more tools that read a Ruby script under a name of their
             * own, and `.rbi` for a Sorbet interface. `.erb` is deliberately
             * absent: an ERB template is text with Ruby in tags, which is a
             * scanner of its own rather than this one. */
            "rb" | "rbw" | "rake" | "gemspec" | "ru" | "podspec" | "jbuilder" | "thor" | "rbi" => {
                Some((Language::Ruby, Dialect::Standard))
            }
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
            /* NOTE: A lock file has no extension of its own to go on, and only some
             * of them are TOML: `Cargo.lock`, `Pipfile`, and the three Python
             * resolvers below are, while `Pipfile.lock` beside `Pipfile` is
             * JSON and is deliberately absent. */
            "cargo.lock" | "pipfile" | "poetry.lock" | "uv.lock" | "pdm.lock" => {
                Some((Language::Toml, Dialect::Standard))
            }
            /* NOTE: YAML owns two extensions, so only the configuration files
             * written with none at all are named here. `.clang-format` and
             * `.clang-tidy` are YAML documents that the LLVM tools read, and
             * `.yamllint` is the linter's own; `.pre-commit-config.yaml` and
             * `.gitlab-ci.yml` carry an extension and are detected by it. */
            ".clang-format" | ".clang-tidy" | ".yamllint" => {
                Some((Language::Yaml, Dialect::Standard))
            }
            /* NOTE: Every one of these is a Ruby script a tool loads by name and
             * evaluates: Bundler's `Gemfile`, Rake's `Rakefile`, and the
             * project files of Guard, Capistrano, Vagrant, Homebrew,
             * CocoaPods, fastlane, Berkshelf, Thor and Danger, plus the two
             * dot files `irb` and `pry` read at start-up. `.gemrc` is
             * deliberately absent: it carries the same air of a Ruby dot file
             * and is a YAML document. */
            "gemfile" | "rakefile" | "guardfile" | "capfile" | "vagrantfile" | "brewfile"
            | "podfile" | "fastfile" | "appfile" | "berksfile" | "thorfile" | "dangerfile"
            | ".irbrc" | ".pryrc" => Some((Language::Ruby, Dialect::Standard)),
            _ => None,
        };
        if let Some((language, dialect)) = reserved {
            return Some(Detection::new(language, dialect, "reserved-filename"));
        }
    }

    let first_line = source.split(|byte| *byte == b'\n').next().unwrap_or(source);
    if first_line.starts_with(b"#!") {
        let line = String::from_utf8_lossy(first_line).to_ascii_lowercase();
        if let Some((_, language, dialect)) =
            SHEBANGS.iter().find(|(name, _, _)| line.contains(name))
        {
            return Some(Detection::new(*language, *dialect, "shebang"));
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
