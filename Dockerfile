# ── Telos ─────────────────────────────────────────────────────────────
#
# CI images for the headless agent graph. The system dependency set is the
# subset of zed's script/linux apt list that the pruned workspace links at
# build time:
#   - libfontconfig-dev: yeslogic-fontconfig-sys (resvg/system-fonts)
#   - cmake: aws-lc-sys (rustls provider)
#   - build-essential + pkg-config: vendored C/C++ build scripts
#   - python3: actus conformance runner (e2e-fake tier)
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
WORKDIR /workspace

COPY . .

# The LLM-free gate: full workspace check plus the graph-crate unit tests.
FROM env AS gate
RUN cargo check --workspace
RUN cargo test -p telos -p external_websocket_sync -p acp_thread -p agent -p agent_servers -p language_models -p icons

# The agent binary used by the deterministic conformance tier (e2e-fake).
FROM env AS tel
RUN cargo build --profile telos-release -p telos
