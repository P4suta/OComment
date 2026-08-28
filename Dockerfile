# syntax=docker/dockerfile:1

# NOTE: `scratch` plus one statically linked musl binary and the two licences:
# NOTE: no shell, no package manager, no libc. docs/docker.md says what a
# NOTE: caller gives up for that, and how a published image is verified.

# NOTE: Docker Hub index digest for the multi-platform rust:1.88-alpine image.
FROM rust:1.88-alpine@sha256:9dfaae478ecd298b6b5a039e1f2cc4fc040fc818a2de9aa78fa714dea036574d AS builder
ARG TARGETARCH
# NOTE: musl-dev is deliberately unpinned: the version that matters is the one
# NOTE: the pinned `rust:1.88-alpine` tag resolves to, and pinning a package
# NOTE: version on top of that only breaks the build when the base image moves.
# hadolint ignore=DL3018
RUN apk add --no-cache musl-dev
WORKDIR /work
COPY . .
# INVARIANT: `docker build .` compiles the CLI here, while the release workflow
# INVARIANT: supplies this whole stage as a buildx named context of binaries it
# INVARIANT: already built, smoke tested, signed, and published as archives. The
# INVARIANT: layout below is the entire contract between the two, which is why
# INVARIANT: the final stage copies from one unchanged line either way and why
# INVARIANT: the released image carries the exact bytes the archives do.
# NOTE: `install -m 0555` because the final stage runs as an unprivileged user
# NOTE: that owns nothing in the image.
RUN cargo build --manifest-path rust/Cargo.toml --release --locked -p ocomment \
    && install -D -m 0555 rust/target/release/ocomment "/out/${TARGETARCH}/ocomment"

FROM scratch
ARG TARGETARCH
COPY --from=builder /out/${TARGETARCH}/ocomment /ocomment
COPY LICENSE-MIT LICENSE-APACHE /licenses/
# NOTE: A numeric id needs no /etc/passwd, which a scratch image has no room for.
USER 65532:65532
# NOTE: The default `check` target is the working directory, so a bare
# NOTE: `docker run -v "$PWD:/src" <image>` checks whatever was mounted.
WORKDIR /src
ENTRYPOINT ["/ocomment"]
CMD ["check"]
