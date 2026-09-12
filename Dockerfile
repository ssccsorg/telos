# ── Telos ─────────────────────────────────────────────────────────────
#
# CI images for the headless agent graph. The system dependency set is the
# subset of zed's script/linux apt list that the pruned workspace links at
# build time:
#   - cmake: aws-lc-sys (rustls provider)
#   - build-essential + pkg-config: vendored C/C++ build scripts
#   - python3: actus conformance runner (e2e-stub tier)
#   - xz-utils: unpack the pinned cargo-chef release archive
# Everything else in zed's list (wayland, x11, alsa, libgit2, sqlite,
# clang/lld, musl, fontconfig, webrtc extras) belongs to stacks this
# workspace prunes.

FROM ubuntu:24.04 AS env

RUN apt-get update && apt-get install -y \
        build-essential \
        pkg-config \
        cmake \
        git \
        curl \
        python3 \
        xz-utils \
    && rm -rf /var/lib/apt/lists/* \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain 1.97.1

ENV PATH="/root/.cargo/bin:${PATH}"
ENV CARGO_INCREMENTAL=0
WORKDIR /workspace

# cargo-chef keys the compile of third-party dependencies on the manifests
# rather than the sources, so a source-only change reuses it. Installed from the
# pinned prebuilt release: `cargo install` compiles the tool's own dependency
# tree, and because this layer sits below every build stage, a miss here also
# discards the cook layers beneath it. Pinned because those layers are derived
# from the skeleton this exact version emits.
#
# Scope of the reuse: cargo-chef discards the compiled dummies of the
# workspace's own members, so only third-party dependencies are carried across
# a source change. The vendored path crates always recompile.
ARG TARGETARCH
RUN set -eu; \
    case "${TARGETARCH:-amd64}" in \
        amd64) chef_target=x86_64-unknown-linux-musl; \
               chef_sha256=aca691abfbfbbe00d482e0ed2249eec3091b65e96a0ce92947fb5b254d48b16d ;; \
        arm64) chef_target=aarch64-unknown-linux-musl; \
               chef_sha256=cd59b90fce5fa84c4189648fec33404ec5491791eebdb812df5e7fca8336408d ;; \
        *) echo "cargo-chef 0.1.78 has no release for TARGETARCH=${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -sSfL --retry 3 --proto '=https' --tlsv1.2 -o /tmp/cargo-chef.tar.xz \
        "https://github.com/LukeMathWalker/cargo-chef/releases/download/v0.1.78/cargo-chef-${chef_target}.tar.xz"; \
    echo "${chef_sha256}  /tmp/cargo-chef.tar.xz" | sha256sum -c -; \
    tar xJf /tmp/cargo-chef.tar.xz -C /tmp; \
    install -m 0755 "/tmp/cargo-chef-${chef_target}/cargo-chef" /usr/local/bin/cargo-chef; \
    rm -rf /tmp/cargo-chef.tar.xz "/tmp/cargo-chef-${chef_target}"; \
    cargo-chef --version

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
# cook has to mirror the commands the gate runs. `cargo check` and `cargo
# build` occupy separate artifact modes, so a build-mode cook is re-checked
# from scratch; and a bare cook would honour [workspace.default-members]
# (crates/telos), leaving the other members' dependencies unbuilt for the
# workspace-wide check.
# The final RUN then drops target so the debug tree stays out of the image.
FROM env AS gate
COPY --from=planner /workspace/recipe.json recipe.json
RUN cargo chef cook --check --workspace --recipe-path recipe.json
# The telos suite links its dependency closure, which needs build-mode units.
RUN cargo chef cook --package telos --recipe-path recipe.json
COPY . .
RUN cargo check --workspace \
    && cargo test -p telos \
    && rm -rf /workspace/target

# The agent binary used by the deterministic conformance tier (e2e-stub).
# cook pre-compiles the release dependencies of the `telos` package; the final
# RUN then rebuilds the workspace members from their real sources.
FROM env AS tel
COPY --from=planner /workspace/recipe.json recipe.json
RUN cargo chef cook --profile telos-release --package telos --recipe-path recipe.json
COPY . .
RUN cargo build --profile telos-release -p telos
