//! Guards that read this crate's own source text.
//!
//! A property test can only find what its generator can draw. These read the
//! generators instead, so an alphabet that is shared today cannot be forked
//! quietly by a line added tomorrow.

use std::collections::BTreeSet;

/// The two files that build sources for a property test, embedded at compile
/// time so the scan does not depend on the directory the test runs in.
///
/// They are the pair `crate::lexical_pool` exists for: the checkpoint and
/// incremental properties inside the crate, and the whole-file properties
/// outside it. Both draw from the same alphabet on purpose, because a fragment
/// worth generating against the whole-file scanner is worth generating against
/// the incremental one.
const GENERATOR_SOURCES: [(&str, &str); 2] = [
    ("src/incremental.rs", include_str!("../src/incremental.rs")),
    ("tests/properties.rs", include_str!("properties.rs")),
];

/// The generators whose arms have to come out of the shared pool, in both
/// files. `lexical_byte` draws one byte and `lexical_fragment` one byte or one
/// whole token, and between them they are the entire alphabet either suite
/// generates from.
const SOURCE_GENERATORS: [&str; 2] = ["lexical_byte", "lexical_fragment"];

/// The one other function allowed to hold a `prop_oneof!`.
///
/// `edit_endpoint` draws an offset into a document rather than a byte of one,
/// so its `Just(0usize)` and `Just(usize::MAX)` arms name positions and not
/// source text; they cannot put a delimiter in front of a scanner and are
/// therefore no part of the alphabet. It is exempt by name so that a *third*
/// generator of source bytes cannot appear beside the two without failing here.
const OFFSET_GENERATORS: [&str; 1] = ["edit_endpoint"];

/// The paths the pool is reachable under. `src/incremental.rs` is inside the
/// crate and `tests/properties.rs` outside it, so the same two constants are
/// spelled differently in the two files and neither spelling may be the only
/// one accepted.
const POOL_PATHS: [&str; 2] = ["crate::lexical_pool::", "lexical_pool::"];

/// The pool constants themselves, which are also the only names an arm may
/// draw from.
const POOLS: [&str; 2] = ["BYTES", "TOKENS"];

/// The single arm allowed to name a byte of its own, and the reason it is:
/// `\n` is already in [`ocomment_core::lexical_pool::BYTES`], and repeating it
/// as an arm doubles its weight rather than adding a byte the pool lacks. Every
/// other literal would be an alphabet one suite has and the other does not.
const NEWLINE_ARM: &str = "Just(b'\\n')";

/// The macro this guard reads.
///
/// It is matched with the bracket that opens its arms rather than on the name
/// alone, so that the prose which merely names it is not read as an
/// invocation. All three of `proptest`'s spellings count: the macro is
/// `macro_rules!`, so `prop_oneof![...]`, `prop_oneof!(...)` and
/// `prop_oneof!{...}` are one and the same invocation and a guard that read
/// only the first would be blind to a generator written with either other.
const MACRO: &str = "prop_oneof!";

/// The brackets an invocation of [`MACRO`] may be written with, paired with
/// what closes each.
const MACRO_BRACKETS: [(char, char); 3] = [('[', ']'), ('(', ')'), ('{', '}')];

/// One arm of a `prop_oneof!`, with the weight and the strategy separated and
/// the line wrapping taken out.
#[derive(Debug)]
struct Arm {
    /// The file the arm was read from.
    file: &'static str,
    /// The generator it belongs to.
    function: String,
    /// The strategy expression, whitespace collapsed to single spaces.
    strategy: String,
}

/// Every arm of the two source generators draws from
/// [`ocomment_core::lexical_pool`], from the uniform-random byte beside it, or
/// is the one `\n` arm that reweights a byte the pool already holds.
///
/// The invariant is textual, and deliberately so: nothing at run time can ask a
/// `Strategy` what alphabet it came from. What a fork would look like is a
/// `Just(b'%')` or a `select(&[...])` added to one file when a language needs a
/// new opener — which is exactly the edit that must go into `lexical_pool`
/// instead, where the other suite gets it too and where the reason for it is
/// written down next to it.
#[test]
fn every_generated_source_byte_comes_from_the_shared_pool() {
    let mut offenders = Vec::new();
    let mut newline_arms = 0;
    let mut pool_arms = 0;
    for arm in source_generator_arms() {
        if arm.strategy == NEWLINE_ARM {
            newline_arms += 1;
            continue;
        }
        if arm.strategy.contains('\'') || arm.strategy.contains('"') {
            offenders.push(format!(
                "{}: `{}` in `{}` spells a literal of its own",
                arm.file, arm.strategy, arm.function
            ));
            continue;
        }
        if draws_from_pool(&arm.strategy) {
            pool_arms += 1;
            continue;
        }
        if arm.strategy.starts_with("any::<") || arm.strategy.starts_with("lexical_byte()") {
            continue;
        }
        offenders.push(format!(
            "{}: `{}` in `{}` draws from neither the pool nor `any`",
            arm.file, arm.strategy, arm.function
        ));
    }
    assert!(
        offenders.is_empty(),
        "these `prop_oneof!` arms build source bytes outside \
         `ocomment_core::lexical_pool`, so the two property suites no longer \
         generate from one alphabet: {offenders:?}"
    );
    // NOTE: A reader that matched nothing would pass the loop above forever.
    assert_eq!(
        newline_arms,
        GENERATOR_SOURCES.len(),
        "each file reweights `\\n` exactly once, and the guard found {newline_arms} such arm(s)"
    );
    assert_eq!(
        pool_arms,
        GENERATOR_SOURCES.len() * POOLS.len(),
        "each file draws from both pools exactly once, and the guard found {pool_arms} such arm(s)"
    );
}

/// Both pool constants are drawn in both files, so neither suite can quietly
/// stop generating whole tokens — the multi-byte openers a single-byte alphabet
/// can never synthesise — while still passing the arm check above.
#[test]
fn both_files_draw_from_both_pools() {
    for (file, source) in GENERATOR_SOURCES {
        for pool in POOLS {
            assert!(
                POOL_PATHS
                    .iter()
                    .any(|path| source.contains(&format!("{path}{pool}"))),
                "{file} never names `lexical_pool::{pool}`"
            );
        }
    }
}

/// Every `prop_oneof!` in the two files sits in a generator this guard knows.
/// A new one somewhere else would be an alphabet the arm check never reads.
#[test]
fn the_guard_reads_every_prop_oneof() {
    let allowed: BTreeSet<&str> = SOURCE_GENERATORS
        .into_iter()
        .chain(OFFSET_GENERATORS)
        .collect();
    let mut found = 0;
    for (file, source) in GENERATOR_SOURCES {
        for at in macro_sites(source) {
            let function = enclosing_function(source, at);
            assert!(
                allowed.contains(function.as_str()),
                "{file}: `prop_oneof!` in `{function}`, which this guard does not read"
            );
            found += 1;
        }
    }
    assert!(
        found >= GENERATOR_SOURCES.len() * SOURCE_GENERATORS.len(),
        "only {found} `prop_oneof!` site(s) were found, fewer than the two generators \
         each of the two files declares"
    );
}

/// Every offset in `source` where [`MACRO`] is invoked, whichever of
/// [`MACRO_BRACKETS`] the invocation is written with. A mention with no bracket
/// behind it is prose and no site.
fn macro_sites(source: &str) -> Vec<usize> {
    source
        .match_indices(MACRO)
        .filter(|(at, _)| {
            source[at + MACRO.len()..]
                .trim_start()
                .starts_with(is_macro_opener)
        })
        .map(|(at, _)| at)
        .collect()
}

/// Whether `character` opens the arms of an invocation.
fn is_macro_opener(character: char) -> bool {
    MACRO_BRACKETS
        .iter()
        .any(|(opener, _)| *opener == character)
}

/// Whether `character` is either half of one of [`MACRO_BRACKETS`], which is
/// what the arm reader counts depth over.
fn bracket_depth(character: char) -> i32 {
    if is_macro_opener(character) {
        1
    } else if MACRO_BRACKETS
        .iter()
        .any(|(_, closer)| *closer == character)
    {
        -1
    } else {
        0
    }
}

/// `proptest` accepts `prop_oneof!` written with any of the three bracket
/// pairs, and this guard has to see all three: a generator added as
/// `prop_oneof!( ... )` that the reader never matched would be an alphabet the
/// arm check never reads *and* would slip past
/// [`the_guard_reads_every_prop_oneof`], which can only complain about the
/// sites it finds.
#[test]
fn the_guard_reads_every_bracket_a_prop_oneof_may_be_written_with() {
    for (opener, closer) in [('[', ']'), ('(', ')'), ('{', '}')] {
        let sample = format!(
            "fn forked() -> impl Strategy<Value = u8> {{\n    \
             prop_oneof!{opener}\n        1 => Just(b'%'),\n        \
             2 => any::<u8>(),\n    {closer}\n}}\n"
        );
        let sites = macro_sites(&sample);
        assert_eq!(
            sites,
            vec![sample.find(MACRO).expect("the sample invokes the macro")],
            "`prop_oneof!{opener}` was not read as an invocation"
        );
        assert_eq!(enclosing_function(&sample, sites[0]), "forked");
        assert_eq!(
            arm_strategies(&sample, sites[0]),
            vec!["Just(b'%')", "any::<u8>()"],
            "`prop_oneof!{opener}` arms were not read"
        );
    }
    // NOTE: Prose that names the macro without invoking it is not a site, which
    // NOTE: is what lets the doc comments in both files go on naming it.
    assert!(macro_sites("/// A pool length as a `prop_oneof!` weight.\n").is_empty());
}

/// Every arm of every source generator, across both files.
fn source_generator_arms() -> Vec<Arm> {
    let mut arms = Vec::new();
    for (file, source) in GENERATOR_SOURCES {
        for at in macro_sites(source) {
            let function = enclosing_function(source, at);
            if !SOURCE_GENERATORS.contains(&function.as_str()) {
                continue;
            }
            for strategy in arm_strategies(source, at) {
                arms.push(Arm {
                    file,
                    function: function.clone(),
                    strategy,
                });
            }
        }
    }
    assert!(!arms.is_empty(), "no `prop_oneof!` arm was read at all");
    arms
}

/// Whether a strategy expression selects one of the shared pools and nothing
/// else. `select` is the only way either generator reaches a pool, so an arm
/// that names a pool without selecting from it is not one of these.
fn draws_from_pool(strategy: &str) -> bool {
    POOL_PATHS.iter().any(|path| {
        POOLS
            .iter()
            .any(|pool| strategy.starts_with(&format!("select({path}{pool})")))
    })
}

/// The name of the function byte `at` falls inside: the last line at or before
/// it that opens a `fn`. Both files declare every generator at the head of its
/// own line, which is what makes this exact rather than a guess.
fn enclosing_function(source: &str, at: usize) -> String {
    let mut name = String::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if offset > at {
            break;
        }
        if let Some(rest) = line.trim_start().strip_prefix("fn ") {
            name = rest
                .split(['(', '<'])
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
        }
        offset += line.len();
    }
    name
}

/// The strategy expression of each arm of the `prop_oneof!` beginning at `at`,
/// with its line wrapping collapsed to single spaces.
///
/// The macro's arms are `weight => strategy`, separated by commas that a
/// generic argument or a nested call may also contain, so the split is made at
/// bracket depth zero and nowhere else. Depth counts all three of
/// [`MACRO_BRACKETS`], because the outermost pair is whichever one the
/// invocation was written with.
fn arm_strategies(source: &str, at: usize) -> Vec<String> {
    let open = at
        + source[at..]
            .find(is_macro_opener)
            .expect("`prop_oneof!` is written with a bracket");
    let mut depth = 0;
    let mut end = open;
    for (index, character) in source[open..].char_indices() {
        depth += bracket_depth(character);
        if depth == 0 && bracket_depth(character) < 0 {
            end = open + index;
            break;
        }
    }
    assert!(end > open, "the `prop_oneof!` at {at} is never closed");
    let mut strategies = Vec::new();
    for arm in split_top_level(&source[open + 1..end]) {
        let arm = arm.trim();
        if arm.is_empty() {
            continue;
        }
        let strategy = arm
            .split_once("=>")
            .unwrap_or_else(|| panic!("`{arm}` is not a `weight => strategy` arm"))
            .1;
        strategies.push(collapsed(strategy));
    }
    assert!(!strategies.is_empty(), "the `prop_oneof!` at {at} is empty");
    strategies
}

/// `body` split on the commas that sit outside every bracket pair.
fn split_top_level(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (index, character) in body.char_indices() {
        depth += bracket_depth(character);
        if character == ',' && depth == 0 {
            parts.push(&body[start..index]);
            start = index + 1;
        }
    }
    parts.push(&body[start..]);
    parts
}

/// One expression with every run of whitespace collapsed to a single space, so
/// an arm that wraps across lines reads as the one expression it is.
fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The two files that raise errors, embedded so the guard does not depend on
/// the directory the test runs in.
const ERROR_SOURCES: [(&str, &str); 2] = [
    ("src/scanner.rs", include_str!("../src/scanner.rs")),
    ("src/profile.rs", include_str!("../src/profile.rs")),
];

/// The two ways this crate names a diagnostic code: the scanner's helper takes
/// it as the first argument, and the profile path builds the struct directly.
const CODE_MARKERS: [&str; 2] = ["self.error(", "code: "];

/// Every code literal that follows one of the markers, wherever the argument
/// sits on the line the call opens or on the next one.
fn codes_raised() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (_, text) in ERROR_SOURCES {
        for marker in CODE_MARKERS {
            let mut rest = text;
            while let Some(at) = rest.find(marker) {
                rest = &rest[at + marker.len()..];
                let Some(open) = rest.find('"') else { break };
                /* NOTE: A marker whose literal is further away than the next
                 * line is not a call this guard can read. The reverse direction
                 * catches a misread: a code nobody found here would show up as
                 * a ledger entry with no raiser. */
                if rest[..open].matches('\n').count() > 1 {
                    continue;
                }
                let literal = &rest[open + 1..];
                let Some(close) = literal.find('"') else {
                    break;
                };
                found.insert(literal[..close].to_owned());
            }
        }
    }
    found
}

/// Every error the scanners raise is classified, and every classification is
/// raised by a scanner.
///
/// The first direction is the one that matters: an error added tomorrow decides
/// how much of a file a forced run may still edit, and `damage` answers `Rest`
/// for a code it does not know. That default is safe, and it is also silent --
/// a new error whose damage is confined to the bytes it names would quietly
/// stop `--force-invalid` from doing its job over everything after it, and
/// nothing would say why. The second direction keeps the ledger from describing
/// a scanner that no longer exists.
#[test]
fn error_codes_are_all_classified() {
    let raised = codes_raised();
    let classified: BTreeSet<String> = ocomment_core::ERROR_CODES
        .iter()
        .map(|&(code, _)| code.to_owned())
        .collect();
    let unclassified: Vec<_> = raised.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "these errors are raised but `ERROR_CODES` does not say how far their damage \
         reaches, so `--force-invalid` would decline to edit anything after them \
         without anyone having decided that: {unclassified:?}"
    );
    let unraised: Vec<_> = classified.difference(&raised).collect();
    assert!(
        unraised.is_empty(),
        "`ERROR_CODES` classifies errors no scanner raises: {unraised:?}"
    );
}

/// The declared order of the severities is the order they claim to be in.
///
/// `ALL` says "most to least severe" and the derived ordering says the same
/// thing from the declaration, and `is_failure` reads both as one question.
/// Two statements of one fact can part; this is what stops them.
#[test]
fn the_severities_are_declared_most_severe_first() {
    use ocomment_core::Severity;
    let mut sorted = Severity::ALL;
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        Severity::ALL,
        "`ALL` and the derived ordering disagree about which severity is worse"
    );
    assert!(Severity::Error.is_failure());
    for severity in Severity::ALL.into_iter().filter(|s| *s != Severity::Error) {
        assert!(
            !severity.is_failure(),
            "{severity} is not an error and was read as one"
        );
    }
}

/// A severity is compared as a category, not as a name.
///
/// `severity == Severity::Error` means "did the scan fail", and that is a
/// question about a category with one member today. The same shape -- a
/// comparison naming one variant while meaning a group -- has been wrong twice
/// in this repository already, once in `OutputFormat` where it reached CI.
#[test]
fn a_severity_is_asked_about_rather_than_named() {
    let offenders: Vec<&str> = ERROR_SOURCES
        .iter()
        .filter(|(_, text)| text.contains("== Severity::") || text.contains("!= Severity::"))
        .map(|(name, _)| *name)
        .collect();
    assert!(
        offenders.is_empty(),
        "these compare a severity by name; `is_failure()` asks the question \
         they mean, in one place: {offenders:?}"
    );
}
