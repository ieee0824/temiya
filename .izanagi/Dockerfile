# syntax=docker/dockerfile:1

# Reuse official toolchain images and keep the final runtime self-contained.
FROM rust:1.99.1-alpine AS rust-toolchain

FROM node:22-alpine AS node-toolchain

FROM izanagi-vm:0.2.4

COPY --from=rust-toolchain /usr/local/cargo /usr/local/cargo
COPY --from=rust-toolchain /usr/local/rustup /usr/local/rustup
COPY --from=node-toolchain /usr/local /usr/local

ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:${PATH}

RUN apk add --no-cache \
        build-base curl github-cli git pkgconf ripgrep

RUN npm install --global \
        @openai/codex@0.164.0 \
        @anthropic-ai/claude-code@2.1.310 \
    && codex --version \
    && claude --version \
    && mv /usr/local/bin/codex /usr/local/bin/codex-daemon

COPY .izanagi/codex-wrapper /usr/local/bin/codex

RUN chmod +x /usr/local/bin/codex \
    && codex --version \
    && rustc --version \
    && cargo --version \
    && gh --version
