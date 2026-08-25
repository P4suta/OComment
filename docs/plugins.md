# Scanner plugins

OComment scanner plugins are WebAssembly components implementing
`spec/ocomment-scanner.wit`. A plugin receives source bytes and scan options and
returns only comment spans and kinds. It cannot edit files. The host rechecks
API version, bounds, ordering, overlap, policy, and every generated edit.

The host exposes no WASI, filesystem, network, clock, random, or imported host
functions. Each invocation receives an input-proportional fuel budget and
explicit memory and instance limits.

```sh
ocomment plugin new my-scanner
cd my-scanner
cargo build --release --target wasm32-unknown-unknown
wasm-tools component new target/wasm32-unknown-unknown/release/my_scanner.wasm \
  -o my-scanner.component.wasm
```

Add local artifacts directly. Remote artifacts require a verified digest and
Sigstore identity and are fetched only by explicit `add` or `update` commands.
Normal scans and LSP sessions are offline.

```sh
ocomment plugin add ./my-scanner.component.wasm --name my-scanner
ocomment plugin add 'https://example.test/my-scanner.wasm' \
  --name my-scanner --sha256 <64-hex-digest> --identity release@example.test
ocomment plugin add 'gh:owner/repository@v1.2.3#my-scanner.wasm' \
  --name my-scanner --sha256 <64-hex-digest> --identity release@example.test
ocomment plugin add 'oci:ghcr.io/owner/my-scanner:v1#my-scanner.wasm' \
  --name my-scanner --sha256 <64-hex-digest> --identity release@example.test
ocomment plugin verify
```

`.ocomment.lock` pins the source, version, SHA-256, signature identity, API, and
capabilities. `plugin update` accepts a new digest only after verifying the
artifact against the identity already pinned in that lock. Artifacts live below
`.ocomment/plugins/`. Route a locked and enabled plugin by extension:

```toml
[plugins]
enabled = ["my-scanner"]
routes = { xyz = "my-scanner" }
memory_mib = 64
instances = 4
fuel_per_byte = 128
```
