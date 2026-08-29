# Installation

Every channel below installs the same binary for the same release. Pick one;
they do not need each other. Examples pin `0.1.1` — use the version you want,
and prefer a full pin over a moving tag wherever a workflow or a tap will
resolve it later.

## From crates.io

```sh
cargo install ocomment --locked
```

`--locked` builds against the dependency versions the release was tested with.
This is the only channel that compiles on your machine, so it needs a Rust
toolchain of 1.88 or newer and takes a few minutes.

## Prebuilt binary with cargo-binstall

```sh
cargo binstall ocomment
```

The crate carries the archive URL, the archive format, and the path of the
binary inside it as `package.metadata.binstall`, so `cargo-binstall` downloads
the release archive for your target instead of compiling anything.

## Release archives

Every release publishes one archive per target:

| Platform | Asset |
| --- | --- |
| Linux x86-64 (glibc) | `ocomment-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 (glibc) | `ocomment-aarch64-unknown-linux-gnu.tar.gz` |
| Linux x86-64 (musl, static) | `ocomment-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 (musl, static) | `ocomment-aarch64-unknown-linux-musl.tar.gz` |
| macOS Intel | `ocomment-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `ocomment-aarch64-apple-darwin.tar.gz` |
| Windows x64 | `ocomment-x86_64-pc-windows-msvc.zip` |

Each archive unpacks into an `ocomment-<target>/` directory holding the binary,
both licences, the README, the `ocomment.1` manual page, and completion scripts
for Bash, Zsh, fish, PowerShell, and Elvish.

```sh
gh release download v0.1.1 --repo P4suta/OComment \
  --pattern 'ocomment-x86_64-unknown-linux-gnu.tar.gz*'
tar -xzf ocomment-x86_64-unknown-linux-gnu.tar.gz
install -m 0755 ocomment-x86_64-unknown-linux-gnu/ocomment ~/.local/bin/ocomment
```

Do not skip [Verifying downloads](verify.md): every archive ships a SHA-256, a
Sigstore signature bundle, and a GitHub build-provenance attestation, and
checking them is three commands.

## Homebrew, Scoop, and WinGet

Every release generates and signs a Homebrew formula (`ocomment.rb`), a Scoop
manifest (`ocomment-scoop.json`), and a WinGet manifest (`ocomment.winget.yaml`)
from the SHA-256 values of the archives that release actually built. They are
attached to the release as assets and can be installed directly:

```sh
brew install --formula ./ocomment.rb
```

```powershell
scoop install .\ocomment-scoop.json
winget install --manifest .\ocomment.winget.yaml
```

There is no published tap, bucket, or WinGet listing yet. Once those exist, the
same generated files are what gets submitted to them, and the commands become
the ordinary `brew install ocomment`, `scoop install ocomment`, and
`winget install OComment.OComment`.

## Container image

```sh
docker run --rm -v "$PWD:/src" ghcr.io/p4suta/ocomment:0.1.1 check
```

The image is `scratch` plus one statically linked musl binary, built from the
exact archives of the same release rather than from a second compilation.
[Docker](docker.md) covers writing files back, exit codes, and running as your
own user.

## GitHub Actions

```yaml
      - uses: P4suta/OComment@v0.1.1
        with:
          paths: src tests
```

The composite action downloads the release archive for the runner, verifies its
SHA-256 and its build-provenance attestation, and annotates the pull request.
[CI and hooks](ci.md) documents every input and output, including SARIF upload
to code scanning.

## pre-commit

```yaml
repos:
  - repo: https://github.com/P4suta/OComment
    rev: v0.1.1
    hooks:
      - id: ocomment-check
```

The hooks are `language: system`, so install the CLI through one of the channels
above first. [CI and hooks](ci.md) explains why, and what `args: ["--staged"]`
changes for a partially staged file.

## Editor extension

The VS Code extension is currently source-only: it has not been published to
the Visual Studio Marketplace, Open VSX, or GitHub Releases. The source under
[`editors/vscode`](https://github.com/P4suta/OComment/tree/main/editors/vscode)
can be built and installed locally, but it has its own version and release
lifecycle. It is a client only and launches the separately installed
`ocomment` binary. Every other LSP client can launch `ocomment lsp` directly —
see [Editors and LSP](editors.md).

## From source

```sh
git clone https://github.com/P4suta/OComment
cd OComment
cargo build --manifest-path rust/Cargo.toml --release --locked -p ocomment
```

The binary lands at `rust/target/release/ocomment`. Building the OCaml reference
implementation and running the differential suite additionally needs OCaml 5.5,
opam, Dune, and Python 3; see
[CONTRIBUTING.md](https://github.com/P4suta/OComment/blob/main/CONTRIBUTING.md).

## Check what you got

```sh
ocomment --version
ocomment doctor
```

`doctor` reports the resolved configuration, the Git integration, the plugin
lock, and the external tools it can find, which is the fastest way to see that
an install is complete and that a hook or an editor will be able to launch it.
