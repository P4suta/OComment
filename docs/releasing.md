# Releasing OComment

Tags named `vMAJOR.MINOR.PATCH` run the release workflow. It builds and smoke
tests Linux x86_64/aarch64 GNU and musl, macOS Intel/Apple Silicon, and Windows
x64 archives. Archives contain the binary, licenses, README, man page, and shell
completions. The workflow builds the VSIX from the same tag, then creates a
draft release containing every archive, VSIX, checksum, SPDX JSON SBOM,
package-manager definition, keyless Sigstore signature, and GitHub
build-provenance attestation.

Before tagging:

1. Update all workspace and internal adapter versions together, update
   `editors/vscode/package.json`, and add the version to both changelogs.
2. Run `./tools/release-check.sh` and confirm the cross-target smoke jobs and
   six expanded-crate artifact checks are green.
3. Confirm the publishing secrets, publisher agreements, GHCR visibility, and
   required environment reviewers from the checklist below are ready.

Publishing is workflow-owned. After the draft exists, crates.io, GHCR, Visual
Studio Marketplace, and Open VSX run as independent retryable jobs using the
already-built artifacts. `tools/publish-crates.sh` reads every package name and
version from Cargo metadata, skips an exact version already visible in the
registry, and resumes in dependency order. Only after all four destinations
succeed does the reviewer-protected `release` environment allow `finalize` to
make the GitHub release public. Do not publish crates by hand between those
jobs; that defeats the resumable state the workflow verifies.

The benchmark workflow is manual-only. Enter the full 40-character commit SHA;
a hosted runner resolves and checks that exact commit before the
reviewer-protected `benchmark` environment lets an ephemeral runner execute it.
The runner uses labels `self-hosted`, `linux`, `x64`, `ocomment-benchmark`, and
`ephemeral`. `tools/release_gate.py` enforces a 20 ms median cold
`--version`, 500 MiB/s for simple scanners, 200 MiB/s for JavaScript and Shell,
a 25 MiB stripped binary, and a maximum 5% regression from the checked-in
machine baseline. If `typos` is installed it also checks that a no-op repository
scan is no slower than 1.5 times `typos`.

Every release also contains signed `ocomment.rb`, `ocomment-scoop.json`, and
`ocomment.winget.yaml` definitions generated from the final archive SHA-256
values. Submit those exact files to the corresponding Homebrew tap, Scoop
bucket, and WinGet repository; they can also be installed directly while an
upstream submission is pending. The CLI crate contains explicit
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
repository can set it. **After the first release, open the package page and set
its visibility to public once.** Until that is done every `docker pull` fails
with an authentication error even though the workflow succeeded. Later releases
inherit the setting.

Renaming the image, dropping an architecture, or moving the `builder` context
layout — `out/amd64/ocomment` and `out/arm64/ocomment` — breaks pinned pulls
and the Dockerfile respectively, so treat both as part of the released
contract, exactly like the archive layout.

## The VS Code extension

`build-vscode` packages `editors/vscode` before the draft is created. The draft
job signs and attests that exact `.vsix`; two independent publish jobs then send
the same bytes to Visual Studio Marketplace and Open VSX. It refuses to run when
`editors/vscode/package.json` does not carry the tag's version — a Marketplace
version can never be republished, so the check has to come before the upload
rather than after. `npm run unit` pins the same file to the workspace crate
version on every pull request, so the two only have to be bumped together.

Three things are manual and are done once, not per release:

1. Create the `P4suta` publisher on the
   [Marketplace management page](https://marketplace.visualstudio.com/manage)
   under an Entra ID tenant, and the matching namespace on
   [Open VSX](https://open-vsx.org/). The `publisher` field in
   `package.json` has to equal the Marketplace publisher id.
2. Create a personal access token for each — an Azure DevOps token with
   **Marketplace: Manage** for the first, an Open VSX access token for the
   second — and store them as the `VSCE_PAT` and `OVSX_PAT` secrets of the
   `vscode-marketplace` environment. They expire; a release that fails at the
   publish step with a 401 usually means one has.
3. Sign the Open VSX publisher agreement. Open VSX rejects the first publish
   until it is signed, and the message says so.

Nothing else in the extension needs a release step: the version, the changelog,
and the README that becomes the Marketplace page are all in `editors/vscode`
and travel with the tag. The Marketplace badge in the repository README stays
grey until the first successful publish.

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

Listing it on the GitHub Marketplace is separate and manual. The first time,
open the release in the GitHub UI and tick **Publish this Action to the GitHub
Marketplace** before publishing the release; the checkbox appears only when
`action.yml` is present at the repository root with a `name`, `description`,
and `branding` block. Later releases inherit the listing, and the Marketplace
version list follows the tags.

Recommend full-version pins such as `P4suta/OComment@v0.1.0` in every example.
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
