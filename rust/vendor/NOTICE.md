# Vendored WebAssembly runtime layers

OComment vendors `wasm_component_layer` 0.1.18 (Apache-2.0) and
`wasm_runtime_layer` 0.4.2 and `wasmi_runtime_layer` 0.31.0
(both MIT OR Apache-2.0). The source is kept unchanged
except for a narrow backend-store accessor and wasmi resource-configuration
methods. Those methods are needed to enforce fuel and StoreLimits while using
the component-model adapter. Upstream repositories:

- https://github.com/DouglasDwyer/wasm_component_layer
- https://github.com/DouglasDwyer/wasm_runtime_layer
