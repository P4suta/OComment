# Internal WebAssembly runtime layers

OComment folds `wasm_component_layer` 0.1.18 (Apache-2.0) and
`wasm_runtime_layer` 0.4.2 and `wasmi_runtime_layer` 0.31.0
(both MIT OR Apache-2.0) into the CLI crate. The internal copy removes unused
standalone-crate serialization surface, narrows visibility, adjusts module
paths, and applies warning-free lint cleanups. Its functional changes remain a
narrow backend-store accessor and wasmi resource-configuration methods needed
to enforce fuel and StoreLimits through the component-model adapter. Upstream
repositories:

- https://github.com/DouglasDwyer/wasm_component_layer
- https://github.com/DouglasDwyer/wasm_runtime_layer
