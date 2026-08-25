# The container image

Every release publishes a multi-architecture image to the GitHub Container
Registry:

```sh
docker run --rm -v "$PWD:/src" ghcr.io/p4suta/ocomment:0.1.0 check
```

`linux/amd64` and `linux/arm64` are built, and both carry the exact binary the
matching `ocomment-<arch>-unknown-linux-musl.tar.gz` release archive contains
— the release workflow pushes the artifacts it already built and smoke tested
rather than compiling the tag a second time.

## What is in the image

A `scratch` base, one statically linked musl binary at `/ocomment`, and
`LICENSE-MIT` and `LICENSE-APACHE` under `/licenses`. There is no shell, no
package manager, and no libc, so nothing else can be run in the container and
`docker exec … sh` has nothing to exec. The entrypoint is the binary itself,
which is why arguments are written as if `ocomment` were on the command line:

```sh
docker run --rm -v "$PWD:/src" ghcr.io/p4suta/ocomment:0.1.0 --version
docker run --rm -v "$PWD:/src" ghcr.io/p4suta/ocomment:0.1.0 diff src >fix.patch
docker run --rm -v "$PWD:/src" ghcr.io/p4suta/ocomment:0.1.0 check --format sarif
```

The working directory is `/src` and the default command is `check`, so a bare
run checks whatever was mounted there. Exit codes, `--format`, and the
`.ocomment.toml` discovery rules are the ones
[the CLI documents](../README.md): a config file inside the mounted tree is
found exactly as it would be on the host.

## Writing files back

The container runs as uid 65532, which owns nothing on the host, so `fix` needs
to be told who to write as:

```sh
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:/src" \
  ghcr.io/p4suta/ocomment:0.1.0 fix src
```

`fix` writes each file through a temporary file beside it, so the process needs
write permission on the containing directory as well as the file. For a
read-only command, mounting read-only makes that explicit and costs nothing:

```sh
docker run --rm -v "$PWD:/src:ro" ghcr.io/p4suta/ocomment:0.1.0 check
```

`fix --interactive` needs a terminal on both standard input and standard
output, so add `-it` when you want it.

## What the image cannot do

`--staged` reads and rewrites Git index blobs by running `git`, and there is no
`git` in the image, so it fails with `--staged needs a Git repository`.
Mounting the host's `.git` directory does not help — the binary still has no
`git` to run. Use the host CLI for staged workflows: `cargo install ocomment
--locked`, a release archive, or the [pre-commit hook](ci.md).

Fetching a plugin from an `https:`, `gh:`, or `oci:` source needs `curl`, `gh`,
or `oras`, and `--identity` verification needs `cosign`; none of them are in
the image either. A plugin already vendored into the mounted tree loads
normally, because that is the binary's own WASM host doing the work. `ocomment
doctor` lists every one of these:

```console
$ docker run --rm -v "$PWD:/src:ro" ghcr.io/p4suta/ocomment:0.1.0 doctor
...
git: not found (needed for --staged)
curl: not found (needed for https:// plugin sources)
gh: not found (needed for gh: plugin sources)
oras: not found (needed for oci: plugin sources)
cosign: not found (needed for --identity verification)
```

## Tags

`0.1.0` pins one release. `0.1` follows the patch releases of that minor
series, and `latest` follows the newest release. Pin the full version in CI,
or pin the digest when the image must never move at all:

```sh
docker run --rm -v "$PWD:/src" ghcr.io/p4suta/ocomment@sha256:… check
```

## Verifying the image

The image is signed keylessly with Sigstore and carries a build-provenance
attestation, both bound to the release workflow of this repository:

```sh
cosign verify ghcr.io/p4suta/ocomment:0.1.0 \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github\.com/P4suta/OComment/\.github/workflows/release\.yml@refs/tags/v[0-9]+\.[0-9]+\.[0-9]+$'
```

The regexp is the point of the check: it accepts only a signature made by
`.github/workflows/release.yml` in `P4suta/OComment` running on a `v*` tag.
A looser identity — anything matching `.*`, say — would accept a signature
from any workflow in any repository and prove nothing.

The provenance attestation is verified with either the GitHub CLI or cosign:

```sh
gh attestation verify oci://ghcr.io/p4suta/ocomment:0.1.0 --repo P4suta/OComment
```

The image is also published with an SPDX SBOM and SLSA provenance attached by
buildx, which `docker buildx imagetools inspect ghcr.io/p4suta/ocomment:0.1.0`
lists.

## Building it yourself

`docker build .` at the repository root compiles the CLI from source in a
`rust:1.88-alpine` stage instead of downloading a release. That is the path CI
exercises on every pull request, so it stays working between releases:

```sh
docker build -t ocomment:dev .
docker run --rm -v "$PWD:/src:ro" ocomment:dev check
```

A source build compiles for the platform being built, so building a foreign
architecture this way runs the compiler under emulation and is slow. The
release workflow avoids that entirely by replacing the builder stage with a
buildx named context holding the already-built binaries:

```sh
docker buildx build --build-context builder=release/binaries \
  --platform linux/amd64,linux/arm64 .
```

where `release/binaries` holds `out/amd64/ocomment` and `out/arm64/ocomment`.
