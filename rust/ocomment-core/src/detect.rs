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

/// How an executable basename is compared with one interpreter name.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Spelling {
    /// The basename is the interpreter name, ignoring ASCII case.
    Exact,
    /// The name, or that basename followed only by a numeric version such as
    /// `python3.12` or `lua5.4`.
    NumericVersion,
}

/// The interpreter names a `#!` line is read for, in the order they are tried,
/// with the language and dialect each one selects.
///
/// Only the executable basename is compared. Interpreter-looking parent
/// directories and arguments are data, not evidence. `env` is handled before
/// this table: its options and assignments are consumed until the executable
/// it will launch is reached.
const SHEBANGS: [(&str, Language, Dialect, Spelling); 20] = [
    (
        "python",
        Language::Python,
        Dialect::Standard,
        Spelling::NumericVersion,
    ),
    ("bash", Language::Shell, Dialect::Bash53, Spelling::Exact),
    ("zsh", Language::Shell, Dialect::Zsh, Spelling::Exact),
    ("luajit", Language::Lua, Dialect::Standard, Spelling::Exact),
    (
        "lua",
        Language::Lua,
        Dialect::Standard,
        Spelling::NumericVersion,
    ),
    ("php", Language::Php, Dialect::Standard, Spelling::Exact),
    (
        "truffleruby",
        Language::Ruby,
        Dialect::Standard,
        Spelling::Exact,
    ),
    ("jruby", Language::Ruby, Dialect::Standard, Spelling::Exact),
    ("ruby", Language::Ruby, Dialect::Standard, Spelling::Exact),
    ("rscript", Language::R, Dialect::Standard, Spelling::Exact),
    ("dart", Language::Dart, Dialect::Standard, Spelling::Exact),
    ("swift", Language::Swift, Dialect::Standard, Spelling::Exact),
    (
        "dotnet-script",
        Language::CSharp,
        Dialect::Standard,
        Spelling::Exact,
    ),
    ("perl", Language::Perl, Dialect::Standard, Spelling::Exact),
    (
        "scala-cli",
        Language::Scala,
        Dialect::Standard,
        Spelling::Exact,
    ),
    ("scala", Language::Scala, Dialect::Standard, Spelling::Exact),
    ("sh", Language::Shell, Dialect::PosixSh, Spelling::Exact),
    (
        "node",
        Language::JavaScript,
        Dialect::Standard,
        Spelling::Exact,
    ),
    (
        "deno",
        Language::JavaScript,
        Dialect::Standard,
        Spelling::Exact,
    ),
    ("r", Language::R, Dialect::Standard, Spelling::Exact),
];

/// The executable one `#!` line actually launches.
///
/// A direct shebang contributes only its first token. When that token is
/// `env`, options and assignments are consumed according to `env`'s command
/// line instead. This deliberately never searches parent directories, option
/// values, assignments, or arguments for an interpreter-looking substring.
fn shebang_executable(line: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(line.strip_prefix(b"#!")?).ok()?;
    let mut words: Vec<String> = text.split_ascii_whitespace().map(str::to_owned).collect();
    let direct = words.first()?;
    if executable_basename(direct) != Some("env") {
        return Some(direct.clone());
    }
    words.remove(0);
    env_executable(words)
}

fn env_executable(mut words: Vec<String>) -> Option<String> {
    let mut index = 0usize;
    while index < words.len() {
        let word = &words[index];
        if word == "--" {
            return words.get(index + 1).cloned();
        }
        if word == "-S" || word == "--split-string" {
            let split = words.get(index + 1..)?.join(" ");
            words = split_env_string(&split)?;
            index = 0;
            continue;
        }
        if let Some(value) = word
            .strip_prefix("--split-string=")
            .or_else(|| word.strip_prefix("-S").filter(|value| !value.is_empty()))
        {
            let mut split = value.to_owned();
            if let Some(rest) = words.get(index + 1..)
                && !rest.is_empty()
            {
                split.push(' ');
                split.push_str(&rest.join(" "));
            }
            words = split_env_string(&split)?;
            index = 0;
            continue;
        }
        if matches!(
            word.as_str(),
            "-u" | "--unset" | "-C" | "--chdir" | "-a" | "--argv0"
        ) {
            index = index.checked_add(2)?;
            continue;
        }
        if ["-u", "-C", "-a"]
            .iter()
            .any(|option| word.starts_with(option) && word.len() > option.len())
            || ["--unset=", "--chdir=", "--argv0="]
                .iter()
                .any(|option| word.starts_with(option))
        {
            index += 1;
            continue;
        }
        if matches!(
            word.as_str(),
            "-i" | "--ignore-environment"
                | "-0"
                | "--null"
                | "-v"
                | "--debug"
                | "--block-signal"
                | "--default-signal"
                | "--ignore-signal"
                | "--list-signal-handling"
        ) || ["--block-signal=", "--default-signal=", "--ignore-signal="]
            .iter()
            .any(|option| word.starts_with(option))
        {
            index += 1;
            continue;
        }
        if word.starts_with('-') {
            /* NOTE: Guessing whether an unknown option consumes the next token can
             * turn its value into an interpreter. Unknown syntax is therefore
             * deliberately undetected. */
            return None;
        }
        if is_env_assignment(word) {
            index += 1;
            continue;
        }
        return Some(word.clone());
    }
    None
}

/// Split the string accepted by `env -S`. This is the small shell-like part of
/// `env`'s interface: ASCII whitespace separates words, quotes group it, and a
/// backslash quotes the following character. An unfinished quote or escape is
/// invalid and fails closed.
fn split_env_string(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote = None;
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            word.push(character);
            started = true;
            escaped = false;
            continue;
        }
        match quote {
            Some(mark) if character == mark => quote = None,
            Some('\'') => {
                word.push(character);
                started = true;
            }
            Some('"') if character == '\\' => escaped = true,
            Some('"') => {
                word.push(character);
                started = true;
            }
            Some(_) => unreachable!("only quote characters are stored"),
            None if character == '\\' => escaped = true,
            None if matches!(character, '\'' | '"') => {
                quote = Some(character);
                started = true;
            }
            None if character.is_ascii_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            None => {
                word.push(character);
                started = true;
            }
        }
    }
    if quote.is_some() || escaped {
        return None;
    }
    if started {
        words.push(word);
    }
    Some(words)
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn executable_basename(executable: &str) -> Option<&str> {
    executable
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
}

fn interpreter_matches(basename: &str, name: &str, spelling: Spelling) -> bool {
    let basename = basename.to_ascii_lowercase();
    match spelling {
        Spelling::Exact => basename == name,
        Spelling::NumericVersion => basename.strip_prefix(name).is_some_and(|suffix| {
            let mut characters = suffix.chars();
            let first = characters.next();
            let last = suffix.chars().next_back();
            suffix.is_empty()
                || (first.is_some_and(|character| character.is_ascii_digit())
                    && last.is_some_and(|character| character.is_ascii_digit())
                    && suffix
                        .chars()
                        .all(|character| character.is_ascii_digit() || character == '.'))
        }),
    }
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
/// These are executable basenames, not substrings to search for in an entire
/// shebang. The Python and Lua entries also accept a numeric version suffix.
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
             * scanner. `.Rmd` is R Markdown and is detected as Markdown, whose
             * fenced-block scan reads its `{r}` chunks as R. */
            "r" => Some((Language::R, Dialect::Standard)),
            /* NOTE: `.dart` is the only suffix Dart owns. `.dart_tool` names the
             * per-package build directory rather than a file, and a
             * `pubspec.yaml` beside it is YAML and is detected as that. */
            "dart" => Some((Language::Dart, Dialect::Standard)),
            /* NOTE: `.swift` is the only suffix Swift owns, and `Package.swift`
             * carries it, so the one file name a Swift package is required to
             * spell exactly needs no reserved-name rule of its own.
             * `.swiftinterface` is deliberately absent: it is a generated
             * module interface rather than a checked-in source file, and
             * `.swiftmodule` beside it is a binary. */
            "swift" => Some((Language::Swift, Dialect::Standard)),
            /* NOTE: `.csx` is a C# script, which `dotnet script` and the C#
             * interactive window read: the same lexical rules with a `#!` line
             * allowed at the first byte and statements at the top level.
             * `.cshtml` and `.razor` are deliberately absent: a Razor page is
             * markup with C# blocks in it, which is a scanner of its own, and
             * `.csproj` beside them is XML. */
            "cs" | "csx" => Some((Language::CSharp, Dialect::Standard)),
            /* NOTE: `.scala` is the language's own suffix and `.sc` the script
             * suffix scala-cli reads, which share the one scanner. `.sbt` is
             * deliberately absent: a build definition is a file of its own
             * with a leading-blank `//` convention that no source file shares,
             * and `.scala.sc` carries `.sc` as its last suffix and is detected
             * as that. */
            "scala" | "sc" => Some((Language::Scala, Dialect::Standard)),
            /* NOTE: `.vue` and `.svelte` are the suffixes of single-file
             * components, whose templates are HTML with code in them and whose
             * script and style bodies are scanned as their own languages.
             * `.scss` and `.sass` are the two Sass syntaxes. They share
             * interpolation and silent comments, but the latter is
             * indentation-based and therefore has its own dialect. */
            /* NOTE: `.md` and `.markdown` are Markdown, and so is `.Rmd` —
             * an R Markdown document, whose `{r}` chunk headers name R for
             * the fenced-block scan — which is what the note that once kept
             * it from the R entry is now the record of. */
            "md" | "markdown" | "rmd" => Some((Language::Markdown, Dialect::Standard)),
            /* NOTE: `.pl`, `.pm` and `.t` are Perl — a program, a module and
             * a test — and so is a `perl` `#!` line. `.pod` is deliberately
             * absent: a POD document is documentation only, with no code to
             * scan. */
            "pl" | "pm" | "t" => Some((Language::Perl, Dialect::Standard)),
            "vue" => Some((Language::Vue, Dialect::Standard)),
            "svelte" => Some((Language::Svelte, Dialect::Standard)),
            "scss" => Some((Language::Css, Dialect::Scss)),
            "sass" => Some((Language::Css, Dialect::Sass)),
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
    if let Some(executable) = shebang_executable(first_line)
        && let Some(basename) = executable_basename(&executable)
        && let Some((_, language, dialect, _)) = SHEBANGS
            .iter()
            .find(|(name, _, _, spelling)| interpreter_matches(basename, name, *spelling))
    {
        return Some(Detection::new(*language, *dialect, "shebang"));
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

    type ExpectedDetection = Option<(Language, Dialect)>;

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

    #[test]
    fn shebang_uses_only_the_executable_basename() {
        let cases: &[(&[u8], ExpectedDetection)] = &[
            (
                b"#!/opt/python/bin/ruby -w\n",
                Some((Language::Ruby, Dialect::Standard)),
            ),
            (
                b"#!/usr/share/swift/usr/bin/swift\n",
                Some((Language::Swift, Dialect::Standard)),
            ),
            (
                b"#!/usr/bin/python3.12 -I\n",
                Some((Language::Python, Dialect::Standard)),
            ),
            (b"#!/opt/python/bin/custom\n", None),
            (b"#!/usr/bin/custom ruby python node\n", None),
            (b"#!/usr/bin/myenv python3\n", None),
            (b"#!/usr/bin/python-wrapper\n", None),
            (b"#!/usr/bin/python3.\n", None),
            (b"#!/usr/bin/bashful\n", None),
        ];
        for (line, expected) in cases {
            let actual = detect_language(None, line)
                .map(|detection| (detection.language, detection.dialect));
            assert_eq!(
                actual,
                *expected,
                "shebang: {}",
                String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn env_options_assignments_and_separator_reach_only_the_command() {
        let cases: &[(&[u8], Language)] = &[
            (
                b"#!/usr/bin/env -i LANG=C -- python3 -I\n",
                Language::Python,
            ),
            (b"#!/usr/bin/env -u python -- ruby -w\n", Language::Ruby),
            (
                b"#!/usr/bin/env --unset=python LUA=perl lua\n",
                Language::Lua,
            ),
            (b"#!/usr/bin/env -C /python -- node\n", Language::JavaScript),
            (b"#!/usr/bin/env PYTHON=python perl -w\n", Language::Perl),
            (
                b"#!/usr/bin/env --argv0=python dotnet-script\n",
                Language::CSharp,
            ),
        ];
        for (line, expected) in cases {
            let detection = detect_language(None, line)
                .unwrap_or_else(|| panic!("did not detect {}", String::from_utf8_lossy(line)));
            assert_eq!(
                detection.language,
                *expected,
                "shebang: {}",
                String::from_utf8_lossy(line)
            );
        }

        for line in [
            b"#!/usr/bin/env PYTHON=python custom ruby\n".as_slice(),
            b"#!/usr/bin/env -u python custom node\n",
            b"#!/usr/bin/env --unknown python\n",
        ] {
            assert!(
                detect_language(None, line).is_none(),
                "an env value or argument was mistaken for a command: {}",
                String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn env_split_string_finds_its_first_command_word() {
        let cases: &[(&[u8], Language)] = &[
            (b"#!/usr/bin/env -S python3 -I\n", Language::Python),
            (b"#!/usr/bin/env --split-string=ruby -w\n", Language::Ruby),
            (
                b"#!/usr/bin/env --split-string=node --no-warnings\n",
                Language::JavaScript,
            ),
            (
                b"#!/usr/bin/env -S LANG=C -- scala-cli shebang\n",
                Language::Scala,
            ),
        ];
        for (line, expected) in cases {
            let detection = detect_language(None, line)
                .unwrap_or_else(|| panic!("did not detect {}", String::from_utf8_lossy(line)));
            assert_eq!(
                detection.language,
                *expected,
                "shebang: {}",
                String::from_utf8_lossy(line)
            );
        }
    }
}
