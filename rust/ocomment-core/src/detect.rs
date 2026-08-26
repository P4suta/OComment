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

/// How a `#!` line is searched for one interpreter name.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Spelling {
    /// Anywhere in the line, which is what every name longer than a letter or
    /// two can afford.
    Anywhere,
    /// As a whole word of the line, delimited by the `/` of a path, white
    /// space, or an option's `-`.
    Word,
}

/// The interpreter names a `#!` line is read for, in the order they are tried,
/// with the language and dialect each one selects.
///
/// The line is searched for a [`Spelling::Anywhere`] name as a substring rather
/// than split into words, because an interpreter arrives written a dozen ways:
/// as a path (`#!/bin/bash`), with a version (`#!/usr/bin/python3.12`), or
/// behind `env` with options (`#!/usr/bin/env -S node --enable-source-maps`).
/// The order is therefore part of the rule and not an accident of listing:
/// `bash` and `zsh` both *contain* `sh`, so each has to be met before it, or
/// every Bash script on disk would be read as POSIX shell. `luajit` contains
/// `lua`, and `jruby` and `truffleruby` contain `ruby`, and all three are
/// listed before the name they contain under the same convention, though those
/// pairs name the same language whichever of the two is met first.
///
/// `r` is the one name a substring cannot find: littler installs R's scripting
/// front end under a single letter, and `/usr/` alone carries an `r`, so
/// searching for it that way would read every `#!/usr/bin/awk` on disk as R.
/// It is a [`Spelling::Word`] instead, and it is listed last so that every name
/// spelled out in full is met before a bare letter is considered at all.
const SHEBANGS: [(&str, Language, Dialect, Spelling); 15] = [
    (
        "python",
        Language::Python,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    ("bash", Language::Shell, Dialect::Bash53, Spelling::Anywhere),
    ("zsh", Language::Shell, Dialect::Zsh, Spelling::Anywhere),
    (
        "luajit",
        Language::Lua,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    ("lua", Language::Lua, Dialect::Standard, Spelling::Anywhere),
    ("php", Language::Php, Dialect::Standard, Spelling::Anywhere),
    (
        "truffleruby",
        Language::Ruby,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    (
        "jruby",
        Language::Ruby,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    (
        "ruby",
        Language::Ruby,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    (
        "rscript",
        Language::R,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    (
        "dart",
        Language::Dart,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    ("sh", Language::Shell, Dialect::PosixSh, Spelling::Anywhere),
    (
        "node",
        Language::JavaScript,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    (
        "deno",
        Language::JavaScript,
        Dialect::Standard,
        Spelling::Anywhere,
    ),
    ("r", Language::R, Dialect::Standard, Spelling::Word),
];

/// Whether `line` carries `name` as a whole word.
///
/// A `#!` line is a path and then arguments, so what separates one word of it
/// from the next is everything a command name is not: the `/` of a path, white
/// space, the `-` of an option, and the `#!` itself. `.`, `_` and `+` stay
/// inside a word, so a version suffix does not split one.
fn carries_word(line: &str, name: &str) -> bool {
    line.split(|character: char| {
        !character.is_ascii_alphanumeric() && !matches!(character, '.' | '_' | '+')
    })
    .any(|word| word == name)
}

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
/// look for and not the whole words to compare against — with the single
/// exception of `r`, which is one letter and is compared against whole words.
/// The order they come back in is the order they must be tried in: `bash` and
/// `zsh` both contain `sh`, so each has to be met before it, or every Bash
/// script on disk would be read as POSIX shell.
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
    SHEBANGS.iter().map(|(name, _, _, _)| *name)
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
            /* NOTE: `.zon` is Zig Object Notation, the data format `@import` and
             * `build.zig.zon` are written in. It is the same lexer with the
             * keywords taken away — the same comments, the same string and
             * multiline string literals — so it is the same scanner, and a
             * `build.zig.zon` is detected by that suffix rather than by name. */
            "zig" | "zon" => Some((Language::Zig, Dialect::Standard)),
            /* NOTE: R is written `.R` about as often as `.r`, and the suffix is
             * folded before it is looked up here, so both reach the same
             * scanner. `.Rmd` is deliberately absent: an R Markdown document is
             * Markdown with R chunks in it, which is a scanner of its own. */
            "r" => Some((Language::R, Dialect::Standard)),
            /* NOTE: `.dart` is the only suffix Dart owns. `.dart_tool` names the
             * per-package build directory rather than a file, and a
             * `pubspec.yaml` beside it is YAML and is detected as that. */
            "dart" => Some((Language::Dart, Dialect::Standard)),
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
            /* NOTE: `.Rprofile` is the R script an R session sources at start-up
             * and the one R file that carries no suffix. `.Renviron` beside it
             * is deliberately absent: it is a table of `name=value` lines that
             * R reads without parsing as code, so a `#` in one means nothing to
             * this scanner. `Rprofile.site` is absent for a second reason — it
             * is the system-wide profile, which lives outside a project and not
             * in a checkout. */
            ".rprofile" => Some((Language::R, Dialect::Standard)),
            _ => None,
        };
        if let Some((language, dialect)) = reserved {
            return Some(Detection::new(language, dialect, "reserved-filename"));
        }
    }

    let first_line = source.split(|byte| *byte == b'\n').next().unwrap_or(source);
    if first_line.starts_with(b"#!") {
        let line = String::from_utf8_lossy(first_line).to_ascii_lowercase();
        if let Some((_, language, dialect, _)) =
            SHEBANGS
                .iter()
                .find(|(name, _, _, spelling)| match spelling {
                    Spelling::Anywhere => line.contains(name),
                    Spelling::Word => carries_word(&line, name),
                })
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
