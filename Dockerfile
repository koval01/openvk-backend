# syntax=docker/dockerfile:1.7
#
# The API is stateless: uploads go straight to the S3 bucket (R2 or Silo),
# Postgres and Redis are external, logs go to stdout. Nothing is written to
# the filesystem, so the container can run with a read-only root.

FROM rust:1-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app

COPY rust-toolchain.toml ./
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && printf 'fn main() {}\n' > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release

COPY build.rs ./
COPY proto ./proto
COPY migrations ./migrations
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    touch src/main.rs \
    && cargo build --release --locked \
    && cp /app/target/release/openvk-backend /tmp/openvk-backend

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /tmp/openvk-backend /usr/local/bin/openvk-backend
USER 65534:65534
EXPOSE 8080
ENV RUST_LOG=info
ENV HOST=0.0.0.0
ENV PORT=8080
ENV STORAGE_BACKEND=s3
ENTRYPOINT ["/usr/local/bin/openvk-backend"]
