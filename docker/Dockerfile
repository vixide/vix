# syntax=docker/dockerfile:1
#
# Try Vix without installing it (T743): a container image that opens the
# `vix` terminal editor on the demo workspace (tui/examples/demo-workspace).
#
#   podman build -f docker/Dockerfile -t vix .
#   podman run --rm -it vix                                # the demo workspace
#   podman run --rm -it -v "$PWD:/work" -w /work vix .     # your own files
#
# The build context is the repository root. The published image, once it
# exists, is ghcr.io/vixide/vix (see .github/workflows/container.yml and
# packaging/README.md). It is not published yet.
#
# Note: Podman provides a Docker-compatible CLI, so `docker` commands also work.

# ---- Build stage: compile `vix` from source with the default features ----
FROM rust:1.99.1-slim-trixie AS build

# The Tree-sitter grammars and other vendored C/C++ code need a C++
# compiler; the slim image only carries gcc.
RUN apt-get update \
 && apt-get install -y --no-install-recommends g++ make pkg-config \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# `--profile dist` is the profile the release archives are built with (see
# Cargo.toml), and the default features are the release's feature set.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --profile dist -p vix-tui \
 && install -D -m 0755 target/dist/vix /out/vix

# ---- Runtime stage: a small Debian with git, as a non-root user ----
FROM debian:trixie-slim

LABEL org.opencontainers.image.title="Vix" \
      org.opencontainers.image.description="Vix, a keyboard-friendly terminal text editor and IDE, opened on a demo workspace" \
      org.opencontainers.image.source="https://github.com/vixide/vix" \
      org.opencontainers.image.licenses="Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only"

# git: the Git menu works on the demo workspace. ca-certificates: HTTPS for
# the HTTP client. less: a pager for git.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git less \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 1000 --shell /bin/bash vix

COPY --from=build /out/vix /usr/local/bin/vix
COPY --chown=vix:vix tui/examples/demo-workspace /home/vix/demo

USER vix
WORKDIR /home/vix/demo

# Make the demo workspace a git repository, so the Git menu has a history.
RUN git init --quiet --initial-branch=main \
 && git add --all \
 && git -c user.name=Vix -c user.email=vix@example.invalid \
        commit --quiet -m "Demo workspace"

ENV TERM=xterm-256color \
    COLORTERM=truecolor \
    LANG=C.UTF-8

ENTRYPOINT ["vix"]
