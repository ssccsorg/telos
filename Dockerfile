# ── Telos ─────────────────────────────────────────────────────────────
#
# CI images for the headless agent graph. The system dependency set is the
# subset of zed's script/linux apt list that the pruned workspace links at
# build time:
#   - libfontconfig-dev: yeslogic-fontconfig-sys (resvg/system-fonts)
#   - cmake: aws-lc-sys (rustls provider)
#   - build-essential + pkg-config: vendored C/C++ build scripts
#   - python3: actus conformance runner (e2e-stub tier)
# Everything else in zed's list (wayland, x11, alsa, libgit2, sqlite,
# clang/lld, musl, webrtc extras) belongs to stacks this workspace prunes.

FROM ubuntu:24.04 AS env

RUN apt-get update && apt-get install -y \
        build-essential \
        pkg-config \
        cmake \
        git \
        curl \
        python3 \
        libfontconfig-dev \
    && rm -rf /var/lib/apt/lists/* \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain 1.97.1

ENV PATH="/root/.cargo/bin:${PATH}"
ENV CARGO_INCREMENTAL=0
WORKDIR /workspace

COPY . .

# The LLM-free gate: the full workspace must compile in telos (every synced
# crate builds here), and unit tests cover only the telos-owned crate.
# Synced crates keep their behavior suites upstream; the absorb procedure is
# VENDORING.md.
# One RUN keeps the multi-GB debug target out of the image (and out of the
# gha layer cache, whose 10GB cap the full debug tree would exceed).
FROM env AS gate
RUN cargo check --workspace \
    && cargo test -p telos \
    && rm -rf /workspace/target

# The agent binary used by the deterministic conformance tier (e2e-stub).
FROM env AS tel
RUN cargo build --profile telos-release -p telos
