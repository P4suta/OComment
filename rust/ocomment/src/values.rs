//! `clap::ValueEnum` wrappers around the core vocabulary enums.
//!
//! The orphan rule forbids implementing `clap::ValueEnum` on the types owned by
//! `ocomment-core`, so every user-facing enum gets a transparent newtype here.
//! Names, aliases, and the variant list all come from the core enum, which stays
//! the single source of truth; this module only adds the per-value help clap
//! needs for `--help`, error messages, and shell completions.

use clap::{ValueEnum, builder::PossibleValue};
use ocomment_core::{CommentKind, Dialect, Language, Layout, Policy};
use std::ops::Deref;

/// Register the canonical spelling plus every accepted alias.
///
/// `clap` matches a command-line value through `PossibleValue::matches`, so an
/// alias that is not registered here is not accepted, however well the core
/// `FromStr` understands it. Core aliases are stored with `-` as the separator;
/// the `_` spelling is registered too so both keep working.
fn possible_value(
    name: &'static str,
    aliases: &'static [&'static str],
    help: &'static str,
) -> PossibleValue {
    let mut value = PossibleValue::new(name).help(help);
    for alias in aliases {
        value = value.alias(*alias);
    }
    for spelling in std::iter::once(&name).chain(aliases) {
        if spelling.contains('-') {
            value = value.alias(spelling.replace('-', "_"));
        }
    }
    value
}

macro_rules! value_enum_wrapper {
    ($name:ident, $inner:ty, $help:expr) => {
        #[doc = concat!("A `clap::ValueEnum` view of [`", stringify!($inner), "`].")]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name(pub $inner);

        impl From<$name> for $inner {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                Self(value)
            }
        }

        impl Deref for $name {
            type Target = $inner;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl ValueEnum for $name {
            fn value_variants<'a>() -> &'a [Self] {
                static VARIANTS: [$name; <$inner>::ALL.len()] = {
                    let mut variants = [$name(<$inner>::ALL[0]); <$inner>::ALL.len()];
                    let mut index = 0;
                    while index < variants.len() {
                        variants[index] = $name(<$inner>::ALL[index]);
                        index += 1;
                    }
                    variants
                };
                &VARIANTS
            }

            fn to_possible_value(&self) -> Option<PossibleValue> {
                let help: fn($inner) -> &'static str = $help;
                Some(possible_value(
                    self.0.as_str(),
                    self.0.aliases(),
                    help(self.0),
                ))
            }
        }
    };
}

value_enum_wrapper!(PolicyArg, Policy, |value| match value {
    Policy::Safe => "Remove ordinary and doc comments; keep preambles and directives",
    Policy::Legal => "Like safe, and keep licence and copyright comments as well",
    Policy::All => "Remove every comment that no keep override protects",
});

value_enum_wrapper!(LayoutArg, Layout, |value| match value {
    Layout::Lines => "Keep the line structure and separate tokens that would otherwise join",
    Layout::Columns => "Pad each removed comment so the following columns do not shift",
    Layout::Compact =>
        "Drop lines that held only a removed comment, and the whitespace it left behind",
});

/* NOTE: The CLI is deliberately stricter than the core `FromStr`, which folds case,
 * dashes, and underscores away before it looks a name up: only the canonical
 * spelling, the pinned aliases, and their underscore variants are registered
 * here, so `--language r-u-s-t` stays an error even though the core accepts it. */
value_enum_wrapper!(LanguageArg, Language, |value| match value {
    Language::Rust => "Rust source files",
    Language::Ocaml => "OCaml implementation and interface files",
    Language::C => "C source and header files",
    Language::Cpp => "C++ source and header files",
    Language::Go => "Go source files",
    Language::Java => "Java source files, including Unicode escape translation",
    Language::JavaScript => "JavaScript modules and scripts, including JSX",
    Language::TypeScript => "TypeScript modules and scripts, including TSX",
    Language::Python => "Python source and stub files",
    Language::Shell => "POSIX sh, Bash, and zsh scripts",
    Language::Html => "HTML documents, including nested script and style elements",
    Language::Css => "CSS stylesheets",
    Language::Jsonc => "JSON with comments, including JSON5",
    Language::Sql => "SQL for every supported database dialect",
    Language::Kotlin => "Kotlin source and script files",
    Language::Toml => "TOML documents, including the lock files written in it",
    Language::Lua => "Lua chunks and LuaRocks rockspecs",
    Language::Yaml => "YAML documents, including the tool configurations written in it",
    Language::Php => "PHP scripts and templates; the inline HTML around the tags is content",
    Language::Unknown => "An undetected language",
});

value_enum_wrapper!(DialectArg, Dialect, |value| match value {
    Dialect::Standard => "The default lexical rules of the language",
    Dialect::Jsx => "JavaScript with JSX elements",
    Dialect::Tsx => "TypeScript with JSX elements",
    Dialect::ObjectiveC => "Objective-C extensions to C",
    Dialect::ObjectiveCpp => "Objective-C++ extensions to C++",
    Dialect::GnuC => "GNU extensions to C",
    Dialect::GnuCpp => "GNU extensions to C++",
    Dialect::Cuda => "CUDA extensions to C++",
    Dialect::PosixSh => "The POSIX shell command language",
    Dialect::Bash53 => "Bash 5.3",
    Dialect::Zsh => "The Z shell",
    Dialect::PostgreSql => "PostgreSQL, with dollar-quoted bodies",
    Dialect::MySql => "MySQL, including its executable versioned comments",
    Dialect::Sqlite => "SQLite",
    Dialect::TSql => "Microsoft Transact-SQL",
    Dialect::Oracle => "Oracle SQL and PL/SQL",
});

value_enum_wrapper!(CommentKindArg, CommentKind, |value| match value {
    CommentKind::Line => "An ordinary comment running to the end of the line",
    CommentKind::Block => "An ordinary delimited comment",
    CommentKind::DocLine => "A documentation comment running to the end of the line",
    CommentKind::DocBlock => "A delimited documentation comment",
    CommentKind::Directive => "A tool or language directive such as a pragma or lint control",
    CommentKind::License => "A licence or copyright preamble",
    CommentKind::HtmlComment => "A DOM-observable HTML comment",
    CommentKind::Shebang => "The interpreter line starting an executable script",
    CommentKind::Encoding => "A source encoding declaration",
    CommentKind::OptimizerHint => "A compiler or database optimizer hint",
    CommentKind::VersionComment => "A MySQL versioned comment that the server executes",
});

#[cfg(test)]
mod tests {
    use super::*;

    /// Every spelling the core enum accepts must reach clap, which matches only
    /// through the registered name and aliases.
    fn round_trip<T>(canonical: &'static str, aliases: &'static [&'static str])
    where
        T: ValueEnum + Copy + PartialEq + std::fmt::Debug,
    {
        let expected = T::from_str(canonical, false)
            .unwrap_or_else(|_| panic!("clap rejects the canonical name `{canonical}`"));
        for spelling in std::iter::once(&canonical).chain(aliases) {
            for candidate in [
                (*spelling).to_owned(),
                spelling.to_ascii_uppercase(),
                spelling.replace('-', "_"),
            ] {
                let parsed = T::from_str(&candidate, true)
                    .unwrap_or_else(|_| panic!("clap rejects `{candidate}`"));
                assert_eq!(
                    parsed, expected,
                    "`{candidate}` resolved to the wrong value"
                );
            }
        }
    }

    #[test]
    fn every_core_spelling_reaches_clap() {
        for value in Policy::ALL {
            round_trip::<PolicyArg>(value.as_str(), value.aliases());
        }
        for value in Layout::ALL {
            round_trip::<LayoutArg>(value.as_str(), value.aliases());
        }
        for value in Language::ALL {
            round_trip::<LanguageArg>(value.as_str(), value.aliases());
        }
        for value in Dialect::ALL {
            round_trip::<DialectArg>(value.as_str(), value.aliases());
        }
        for value in CommentKind::ALL {
            round_trip::<CommentKindArg>(value.as_str(), value.aliases());
        }
    }

    #[test]
    fn every_value_carries_help_and_the_core_variant_order() {
        assert_eq!(
            LanguageArg::value_variants()
                .iter()
                .map(|value| value.0)
                .collect::<Vec<_>>(),
            Language::ALL.to_vec()
        );
        for value in DialectArg::value_variants() {
            let possible = value
                .to_possible_value()
                .expect("dialects are never hidden");
            assert_eq!(possible.get_name(), value.0.as_str());
            assert!(
                possible.get_help().is_some(),
                "`{}` has no help text",
                value.0
            );
        }
    }

    #[test]
    fn punctuated_aliases_survive() {
        assert_eq!(
            LanguageArg::from_str("c++", false),
            Ok(LanguageArg(Language::Cpp))
        );
        assert_eq!(
            DialectArg::from_str("objective-c++", false),
            Ok(DialectArg(Dialect::ObjectiveCpp))
        );
        assert_eq!(
            DialectArg::from_str("bash-5.3", false),
            Ok(DialectArg(Dialect::Bash53))
        );
    }
}
