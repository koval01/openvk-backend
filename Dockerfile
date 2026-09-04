# syntax=docker/dockerfile:1.7

FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && printf 'fn main() {}\n' > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release

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
ENV STORAGE_BACKEND=disk
ENV MEDIA_ROOT=/var/openvk/media
ENTRYPOINT ["/usr/local/bin/openvk-backend"]
