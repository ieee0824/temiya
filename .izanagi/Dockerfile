# syntax=docker/dockerfile:1

# Reuse official toolchain images and keep the final runtime self-contained.
FROM rust:1.93-slim AS rust-toolchain

FROM node:22-bookworm-slim AS node-toolchain

FROM izanagi-vm:latest

COPY --from=rust-toolchain /usr/local/cargo /usr/local/cargo
COPY --from=rust-toolchain /usr/local/rustup /usr/local/rustup
COPY --from=node-toolchain /usr/local /usr/local

ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:${PATH}

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential curl git pkg-config ripgrep \
    && rm -rf /var/lib/apt/lists/*

RUN npm install --global \
        @openai/codex@0.164.0 \
        @anthropic-ai/claude-code@2.1.310 \
    && ln -s /usr/local/bin/node /usr/bin/node \
    && ln -s /usr/local/bin/codex /usr/bin/codex \
    && ln -s /usr/local/bin/claude /usr/bin/claude \
    && codex --version \
    && claude --version

RUN rustc --version && cargo --version
