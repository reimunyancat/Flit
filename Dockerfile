# ---- build ----
FROM rust:1-slim-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
RUN cargo build --release

# ---- runtime ----
FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/flit-server /usr/local/bin/flit-server
EXPOSE 8000
CMD ["flit-server"]