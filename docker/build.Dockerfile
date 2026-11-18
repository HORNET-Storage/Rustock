# syntax=docker/dockerfile:1

# Keep in step with rust-toolchain.toml.
ARG RUST_VERSION=1.93.1

# The binary links this image's glibc and libstdc++; docker/runtime.Dockerfile
# must stay on the same Debian release.
FROM rust:${RUST_VERSION}-bookworm AS build

# librocksdb-sys compiles C++, and zstd-sys runs bindgen, which needs libclang.
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential clang libclang-dev \
 && rm -rf /var/lib/apt/lists/*

# Stops rustup from fetching the components listed in rust-toolchain.toml.
ARG RUST_VERSION
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION}

WORKDIR /src
COPY . .

# Comma-separated cargo features for rustock-cli, e.g. "smtp".
ARG FEATURES=""
# Maintenance tools: crates/cli/examples/<name>.rs, shipped beside the binary.
ARG TOOLS="rewind_to set_exec_head reset_exec_head block_info height_peek"
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    examples=""; for t in $TOOLS; do examples="$examples --example $t"; done; \
    cargo build --release --locked -p rustock-cli ${FEATURES:+--features "$FEATURES"} $examples \
 && install -D target/release/rustock-cli /out/rustock-cli \
 && for t in $TOOLS; do install -D "target/release/examples/$t" "/out/tools/$t"; done


FROM scratch AS binary
COPY --from=build /out/rustock-cli /rustock-cli

FROM scratch AS tools
COPY --from=build /out/tools/ /
