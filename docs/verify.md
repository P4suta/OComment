# Verifying downloads

Every release asset is published with three independent pieces of evidence: a
SHA-256 digest, a keyless Sigstore signature, and a GitHub build-provenance
attestation. They answer different questions, so it is worth knowing which one
you are relying on.

| Evidence | Answers |
| --- | --- |
| `SHA256SUMS` and the per-archive `.sha256` | Did the bytes arrive intact? |
| `*.sigstore.json` (cosign) | Was this file signed by this repository's release workflow? |
| Build provenance (`gh attestation`) | Which workflow run, from which commit, produced it? |

A digest alone proves nothing about origin: whoever could replace the archive
could replace the digest beside it. The signature and the attestation are what
tie the file to `P4suta/OComment` and to the tag it claims to come from.

Examples pin `0.1.0` — use the version you want, and change it in every line of
a command: the tag inside a signing identity is part of what the check proves,
not a detail of the example.

## Download

```sh
gh release download v0.1.0 --repo P4suta/OComment \
  --pattern 'ocomment-x86_64-unknown-linux-gnu.tar.gz*' \
  --pattern 'SHA256SUMS*'
```

The trailing `*` in the first pattern brings the archive's `.sha256` and its
`.sigstore.json` bundle along with the archive itself.

## Check the digest

```sh
sha256sum --ignore-missing --check SHA256SUMS
```

`SHA256SUMS` covers every archive, per-archive checksum, the SPDX SBOM, and the
generated Homebrew, Scoop, and WinGet definitions of that release, so
`--ignore-missing` is what lets it pass when you downloaded one of them. The
combined checksum file itself is signed and attested. On macOS the command is
`shasum -a 256`, and in PowerShell it is `Get-FileHash`.

## Check the provenance attestation

```sh
gh attestation verify ocomment-x86_64-unknown-linux-gnu.tar.gz \
  --repo P4suta/OComment
```

This is the shortest honest check, because `gh` resolves the trust root itself.
It succeeds only for an archive built by a workflow run in this repository, and
it prints the workflow and the commit that produced it. The composite GitHub
Action runs this same verification on the runner before it uses the binary.

## Check the signature

```sh
cosign verify-blob \
  --bundle ocomment-x86_64-unknown-linux-gnu.tar.gz.sigstore.json \
  --certificate-identity \
    'https://github.com/P4suta/OComment/.github/workflows/release.yml@refs/tags/v0.1.0' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  ocomment-x86_64-unknown-linux-gnu.tar.gz
```

The identity is the signing workflow, not a person: releases are signed
keylessly by `.github/workflows/release.yml` running on the tag, and there is no
private key anywhere to be stolen. Pin the exact tag as above when you know
which version you are installing. A script that accepts any released version
wants the pattern instead:

```sh
  --certificate-identity-regexp \
    '^https://github\.com/P4suta/OComment/\.github/workflows/release\.yml@refs/tags/v[0-9]+\.[0-9]+\.[0-9]+$'
```

Do not relax that expression to match any ref. `@refs/tags/v...` is the part
that says the signature came from a released tag rather than from a branch or a
pull request, and the release-tag ruleset is what makes those tags immutable.

The same command verifies any other signed asset of the release by name: the
SPDX SBOM `ocomment.spdx.json`, the generated `ocomment.rb`,
`ocomment-scoop.json`, and `ocomment.winget.yaml` definitions, the `SHA256SUMS`
file itself, and each per-archive checksum.

## Verify the container image

```sh
cosign verify ghcr.io/p4suta/ocomment:0.1.0 \
  --certificate-identity-regexp \
    '^https://github\.com/P4suta/OComment/\.github/workflows/release\.yml@refs/tags/v[0-9]+\.[0-9]+\.[0-9]+$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com

gh attestation verify oci://ghcr.io/p4suta/ocomment:0.1.0 --repo P4suta/OComment
```

The image carries the same musl binary the matching release archive contains,
because the release workflow feeds the already-built archives to the image build
rather than compiling the tag a second time. Verifying the archive and verifying
the image therefore vouch for the same bytes.

## If a check fails

Do not install the file. A digest mismatch is usually a truncated download and
is worth retrying once. A signature or attestation failure is not: report it
through the process in
[SECURITY.md](https://github.com/P4suta/OComment/blob/main/SECURITY.md) rather
than opening a public issue.
