# syntax=docker/dockerfile:1

# Reuse official toolchain images and keep the final runtime self-contained.
FROM rust:1.99.1-slim AS rust-toolchain

FROM node:22-bookworm-slim AS node-toolchain

FROM izanagi-vm-debian:0.2.5

COPY --from=rust-toolchain /usr/local/cargo /usr/local/cargo
COPY --from=rust-toolchain /usr/local/rustup /usr/local/rustup
COPY --from=node-toolchain /usr/local /usr/local

ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:${PATH}

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential curl gh git pkg-config ripgrep \
    && rm -rf /var/lib/apt/lists/*

RUN npm install --global \
        @openai/codex@0.164.0 \
        @anthropic-ai/claude-code@2.1.310 \
    && codex --version \
    && claude --version \
    && mv /usr/local/bin/codex /usr/local/bin/codex-daemon

COPY .izanagi/codex-wrapper /usr/local/bin/codex
COPY .izanagi/cargo-wrapper /usr/local/bin/cargo
COPY .izanagi/rustc-wrapper /usr/local/bin/rustc

RUN chmod +x /usr/local/bin/codex /usr/local/bin/cargo /usr/local/bin/rustc \
    && codex --version \
    && rustc --version \
    && cargo --version \
    && gh --version
