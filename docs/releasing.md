# Releasing OComment

Tags named `vMAJOR.MINOR.PATCH` run the release workflow. It builds and smoke
tests Linux x86_64/aarch64 GNU and musl, macOS Intel/Apple Silicon, and Windows
x64 archives. Archives contain the binary, licenses, README, man page, and shell
completions. The publish job creates checksums, an SPDX JSON SBOM, keyless
Sigstore signatures, and GitHub build-provenance attestations before uploading
the release assets.

Before tagging:

1. Confirm the `ocomment` crate name and every internal adapter name are still
   available on crates.io.
2. Update all workspace and internal adapter versions together, and
   `editors/vscode/package.json` with them.
3. Run `./tools/release-check.sh` on the fixed benchmark runner.
4. Confirm the cross-target smoke jobs and package dry runs are green.
5. Publish crates in dependency order: runtime layer, wasmi adapter, component
   layer, core, plugin SDK, then CLI.

The fixed runner uses labels `self-hosted`, `linux`, `x64`, and
`ocomment-benchmark`. `tools/release_gate.py` enforces a 20 ms median cold
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
`linux/arm64` after the GitHub release exists. It does not rebuild the tag: it
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

`publish-vscode` packages `editors/vscode` after the GitHub release exists,
signs the `.vsix` with cosign, attaches both to the release, and publishes to
the Visual Studio Marketplace and to Open VSX. It refuses to run when
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
combined `SHA256SUMS`, and — when `gh` is on the runner — the build-provenance
attestation. Renaming a release asset, dropping a musl target, or changing the
`ocomment-<target>/` leading directory breaks every pinned workflow, so treat
the archive layout as part of the released contract.
