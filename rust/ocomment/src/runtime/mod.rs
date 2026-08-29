//! Internal WebAssembly component runtime.
//!
//! These modules fold the narrowly patched runtime stack into the CLI package,
//! so crates.io users do not need separate OComment implementation crates.

#[path = "wasm_component_layer/lib.rs"]
pub mod wasm_component_layer;
#[path = "wasm_runtime_layer/lib.rs"]
pub mod wasm_runtime_layer;
#[path = "wasmi_runtime_layer/lib.rs"]
pub mod wasmi_runtime_layer;
