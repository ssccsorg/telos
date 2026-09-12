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

# cargo-chef turns the dependency compile into a layer keyed on the
# manifests rather than the sources, so a source-only change reuses it.
# Installed above the source copy so the tool itself is cached too.
RUN cargo install cargo-chef --locked

# The recipe is the dependency graph without the sources. The planner sees
# the full context; the build stages copy only its recipe so their
# dependency layer survives source changes.
FROM env AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# The LLM-free gate: the full workspace must compile in telos (every synced
# crate builds here), and unit tests cover only the telos-owned crate.
# Synced crates keep their behavior suites upstream; the absorb procedure is
# VENDORING.md.
# cook compiles the workspace dependencies into target/debug so the workspace
# check reuses them; the final RUN then compiles only the changed crates and
# drops target so the debug tree stays out of the image.
FROM env AS gate
COPY --from=planner /workspace/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json
COPY . .
RUN cargo check --workspace \
    && cargo test -p telos \
    && rm -rf /workspace/target

# The agent binary used by the deterministic conformance tier (e2e-stub).
# cook pre-compiles the release dependencies of the `telos` package; the
# final RUN then only builds telos and its changed path dependencies.
FROM env AS tel
COPY --from=planner /workspace/recipe.json recipe.json
RUN cargo chef cook --profile telos-release --package telos --recipe-path recipe.json
COPY . .
RUN cargo build --profile telos-release -p telos
