# syntax=docker/dockerfile:1
FROM rust:1.91-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY tools/load-tps tools/load-tps
RUN cargo build --release -p fractal-node

FROM debian:bookworm
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/fractal-node /usr/local/bin/fractal-node
ENV FRACTAL_RPC_ADDR=0.0.0.0:8545
EXPOSE 8545
ENTRYPOINT ["/usr/local/bin/fractal-node"]
