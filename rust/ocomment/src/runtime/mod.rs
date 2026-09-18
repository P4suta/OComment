//! Internal WebAssembly component runtime.
//!
//! These modules fold the narrowly patched runtime stack into the CLI package,
//! so crates.io users do not need separate OComment implementation crates.
//!
//! The one place in this crate that suppresses a lint, and the reason is that
//! this is not our code. The exhaustive-match rule is a rule about decisions
//! this program makes; rewriting twenty match arms of an upstream runtime to
//! satisfy it would put a patch between us and every version of it we take
//! next. `rust_sources_do_not_suppress_lints` names this directory and nothing
//! else, and fails if a second name appears.
//!
//! `expect` rather than `allow`: if a version we take next has no wildcard arm
//! left, this line stops being true and the build says so, instead of sitting
//! here silencing nothing for the rest of the repository's life. A suppression
//! that has outlived its subject is indistinguishable from one that is still
//! working, which is the same failure as a gate nobody watches refuse.
#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "upstream-derived; a rule about this program's own decisions does not reach it"
)]

#[path = "wasm_component_layer/lib.rs"]
pub mod wasm_component_layer;
#[path = "wasm_runtime_layer/lib.rs"]
pub mod wasm_runtime_layer;
#[path = "wasmi_runtime_layer/lib.rs"]
pub mod wasmi_runtime_layer;
