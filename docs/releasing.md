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
2. Update all workspace and internal adapter versions together.
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
