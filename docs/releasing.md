# Releasing OComment

Tags named `vMAJOR.MINOR.PATCH` run the release workflow. It builds and smoke
tests Linux x86_64/aarch64 GNU and musl, macOS Intel/Apple Silicon, and Windows
x64 archives. Archives contain the binary, licenses, README, man page, and shell
completions. It creates a draft release containing every archive and checksum,
an SPDX JSON SBOM, package-manager definitions, keyless Sigstore signatures,
and GitHub build-provenance attestations. The workflow publishes the CLI to
GitHub Releases and crates.io and publishes its container to GHCR; it does not
build or publish a VS Code extension.

Release preparation is automated by `.github/workflows/release-pr.yml`. On a
push to `main`, release-plz compares the three product crates with crates.io and
opens or refreshes one draft Release PR. It updates their shared workspace
version, internal dependency requirements, `Cargo.lock`, and the root CLI
changelog. The workflow then synchronizes the stable version pins in user docs,
regenerates the man page and completions, and explicitly dispatches CI, docs,
and CodeQL for the bot-created branch. Pull-request runs caused by the default
Actions token can require approval; `workflow_dispatch` is an explicit
`GITHUB_TOKEN` recursion exception, so the dispatch makes the required checks
independent of a separate PAT and of that approval queue.

Release-plz deliberately does not publish a crate, create a tag, or create a
GitHub Release here. Those operations remain owned by the signed-tag workflow
and its final Environment approval. `release-plz.toml` and
`tools/check_ci_contracts.py` both enforce that separation.

Before tagging:

1. Review the draft Release PR, including its proposed SemVer change and
   changelog. Mark it ready and merge it only after its dispatched checks pass.
   `editors/vscode/package.json` and its changelog are deliberately independent
   and are not CLI release inputs.
2. On the merged `main`, run `./tools/release-check.sh` and confirm the
   cross-target smoke jobs and
   three expanded-crate artifact checks are green.
3. Confirm `HEAD` is a clean, signed commit equal to `origin/main`, version
   `MAJOR.MINOR.PATCH` is still unused by all three crates, and neither its tag
   nor its GitHub Release exists.
4. Confirm the crates.io Trusted Publisher entries, GHCR visibility plan, and
   required `release` Environment reviewer from the checklist below are ready.

Publishing is workflow-owned. After the draft exists, crates.io and GHCR run as
independent retryable jobs using the tag and already-built artifacts.
`tools/publish-crates.sh` reads all three package names and versions from Cargo
metadata, skips an exact version already visible in the registry, and resumes
in dependency order. Only after both destinations succeed does the
reviewer-protected `release` Environment allow `finalize` to make the GitHub
release public. Do not publish crates by hand between those jobs; that defeats
the resumable state the workflow verifies.

## Published crate boundary

The product has three intentionally public crates: `ocomment`,
`ocomment-core`, and `ocomment-plugin-sdk`. Release-plz manages exactly these
three as one version group, and only the CLI owns the release changelog.

The CLI used to publish three implementation-support forks:
`ocomment-wasm-runtime-layer`, `ocomment-wasmi-runtime-layer`, and
`ocomment-wasm-component-layer`. Their narrowly patched implementations now live
as private modules inside `ocomment`, so releases publish only the three product
crates in dependency order. The support-fork versions already used by
`ocomment 0.1.0` remain immutable registry history and must not be yanked: doing
so would break resolution for that release. Do not publish new versions of those
support crates.

## One-time publishing setup

1. In the crates.io settings for each of `ocomment`, `ocomment-core`, and
   `ocomment-plugin-sdk`, add a GitHub Trusted Publisher with owner `P4suta`,
   repository `OComment`, workflow filename `release.yml`, and environment
   `crates-io`. The release job uses GitHub OIDC to obtain a short-lived token;
   it does not read a registry secret. After verifying the first trusted
   publication, revoke the old crates.io API token and delete the now-unused
   `CARGO_REGISTRY_TOKEN` Environment secret. See the
   [crates.io Trusted Publishing documentation](https://crates.io/docs/trusted-publishing).
2. If this repository restricts Actions to an allowlist, permit the official
   `rust-lang/crates-io-auth-action` at the SHA pinned in `release.yml`.
3. Configure the `release` Environment with `P4suta` as a required reviewer.
   The approval is intentionally the last gate: leave `finalize` waiting until
   the registries and draft assets have been verified.
4. The first GHCR package is private. As soon as `publish-container` creates
   it, change the package visibility to public before approving `release`.

## Release sequence

1. Fetch `origin/main` and tags, verify a clean tree and signed `HEAD`, and run
   the metadata and release gates one final time on the exact commit. Check that
   `vMAJOR.MINOR.PATCH` and its GitHub Release do not exist and that all three
   target crate versions are unused.
2. Create and verify a signed annotated `vMAJOR.MINOR.PATCH` tag on that
   commit, then push only that tag ref. Do not move or recreate a release tag.
3. Monitor the Release workflow. Before either registry result is accepted, it
   must have built and smoke-tested seven target archives, generated the SBOM,
   checksums, signatures and attestations, published three crates in dependency
   order, and pushed the amd64/arm64 image.
4. Make the new GHCR package public. Inspect its multi-platform manifest,
   verify its cosign signature and GitHub attestation, then run `--version` and
   a sample scan from the image.
5. After crates.io propagation, install into a clean temporary prefix with
   `cargo install ocomment --version MAJOR.MINOR.PATCH --locked`. Exercise detection,
   `diff`, `fix --dry-run`, and `fix`, and confirm the installed binary reports
   `ocomment MAJOR.MINOR.PATCH`.
6. While `finalize` waits for approval, download the authenticated draft
   assets. Verify `SHA256SUMS`, each Sigstore bundle, GitHub provenance, and the
   unpacked binaries as described in [Verifying downloads](verify.md).
7. Approve the `release` Environment only after the preceding checks pass.
   Confirm that `finalize` publishes the existing draft as the latest GitHub
   Release without replacing its assets.
8. From an external-user path, repeat a GitHub Release download, `cargo
   install`, GHCR pull, and a workflow using
   `P4suta/OComment@vMAJOR.MINOR.PATCH`.

The benchmark workflow is manual-only. Select the branch or tag to benchmark in
the workflow's **Run workflow** ref picker, then enter that ref's full
40-character commit SHA. A hosted runner requires the input SHA, the immutable
workflow-dispatch SHA, and the checked-out commit to agree before the
reviewer-protected `benchmark` environment lets an ephemeral runner execute it.
The runner uses labels `self-hosted`, `linux`, `x64`, `ocomment-benchmark`, and
`ephemeral`. `tools/release_gate.py` enforces a 20 ms median cold
`--version`, 500 MiB/s for simple scanners, 200 MiB/s for JavaScript and Shell,
a 25 MiB stripped binary, and a maximum 5% regression from the checked-in
machine baseline. If `typos` is installed it also checks that a no-op repository
scan is no slower than 1.5 times `typos`.

Every release also contains signed `ocomment.rb`, `ocomment-scoop.json`, and
`ocomment.winget.yaml` definitions generated from the final archive SHA-256
values. They can be installed directly or submitted unchanged to a future
Homebrew tap, Scoop bucket, and WinGet repository. Creating those upstream
listings is outside the CLI release. The CLI crate contains explicit
`cargo-binstall` URL, archive-format, and in-archive binary metadata for the
same target-qualified archives.

## The container image

`publish-container` pushes `ghcr.io/p4suta/ocomment` for `linux/amd64` and
`linux/arm64` after the draft release exists. It does not rebuild the tag: it
downloads the two `*-unknown-linux-musl` archives the matrix already produced
and feeds them to the Dockerfile through the buildx named context `builder`, so
the image and the archives contain the same bytes. The image is tagged
`MAJOR.MINOR.PATCH`, `MAJOR.MINOR`, and `latest`, signed with cosign, and given
a build-provenance attestation pushed to the registry.

A GHCR package is private when it is first created, and the visibility setting
belongs to the package rather than the repository, so nothing in this
repository can set it. **During the first release, open the package page and
set its visibility to public before approving the `release` Environment.**
Until that is done every unauthenticated `docker pull` fails even though the
workflow succeeded. Later releases inherit the setting.

Renaming the image, dropping an architecture, or moving the `builder` context
layout — `out/amd64/ocomment` and `out/arm64/ocomment` — breaks pinned pulls
and the Dockerfile respectively, so treat both as part of the released
contract, exactly like the archive layout.

## The VS Code extension

The extension remains under `editors/vscode`, and the ordinary `vscode` CI job
still lints, compiles, unit-tests, drives a real VS Code instance, and packages
a test VSIX. It is source-only and is not an asset or publication target of the
CLI release: the release workflow does not checksum, sign, attest, or
attach a VSIX and has no Visual Studio Marketplace or Open VSX jobs.

The extension's manifest version and changelog are independent of the Rust
workspace version and CLI tag. A future extension release needs its own review,
credentials, workflow, verification contract, and publication documentation;
none should be inferred from a successful CLI release.

## The documentation site

`.github/workflows/docs.yml` builds this book with mdBook and publishes it to
GitHub Pages at <https://p4suta.github.io/OComment/>. It is not tied to a
release: the `docs` job runs on every pull request as a required check, and the
`deploy-pages` job runs on every push to `main` that touches `docs/`, `spec/`,
or the generator, so the site follows the default branch rather than the tags.

Four pages are generated by `tools/gen_docs.py` from the built binary and from
`spec/`, and `python3 tools/gen_docs.py --check` runs in the `rust` job of CI as
well as in the docs build, so a change to the CLI's `--help`, to the language
table, or to the directive table fails the build until the pages are
regenerated with `python3 tools/gen_docs.py`.

One thing is manual and is done once, not per release:

**Open Settings → Pages and set the source to "GitHub Actions".** Until that is
done, `deploy-pages` fails on `main` with a Pages API error even though the
build succeeded, and the site stays unpublished. The default source is a branch,
and nothing in this repository can change it. Every link into the site from the
README, from `rust/ocomment/Cargo.toml`, and from this book assumes it has been
set.

The SARIF `helpUri` of every rule still points at the README anchor in this
repository rather than at the site; moving it is a code change, and a separate
one.

## The published GitHub Action

`action.yml` at the repository root is released with the source, so
`P4suta/OComment@vMAJOR.MINOR.PATCH` resolves as soon as the tag exists; no
extra publishing step is needed for it to work in a workflow.

Listing it on the GitHub Marketplace is separate and manual and would require
its own review. GitHub Action Marketplace listing is outside the CLI
release; do not couple it to approval of the CLI's GitHub Release.

Recommend full-version pins such as `P4suta/OComment@vMAJOR.MINOR.PATCH` in
every example.
The release-tag ruleset forbids deleting or force-moving `v*`, so a published
tag never changes underneath a workflow, and there is deliberately no moving
`v0` or `v0.1` tag to maintain. A release that changes the action's inputs,
outputs, or verdict rules is therefore a version bump like any other, and
`docs/ci.md` documents the surface those pins are buying.

The action downloads `ocomment-<target>.tar.gz` (or the Windows `.zip`), the
combined `SHA256SUMS`, and the build-provenance attestation. Verification fails
closed when `gh` is unavailable unless the caller explicitly opts out with
`verify-attestation: false`. Renaming a release asset, dropping a musl target, or changing the
`ocomment-<target>/` leading directory breaks every pinned workflow, so treat
the archive layout as part of the released contract.
