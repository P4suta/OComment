# Shared fixtures

`v1/builtins.json` is consumed unchanged by the Rust/OCaml differential runner.
Every source is encoded to UTF-8 bytes only after JSON parsing; byte offsets are
then compared by the normalized JSONL protocol. The harness runs both `safe`
and `all` policies for every built-in language and adds binary, dialect,
malformed-input, external-span, and declarative-profile cases.

Fixture changes are specification changes. Add the corresponding official
lexical-spec reference and expected behavior to the case note before changing a
source form.
