FROM rust:1.78-slim AS builder
WORKDIR /app

# Cache dependencies (dummy source for layer caching)
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs && echo '' > src/lib.rs
# Limit parallelism and linker memory consumption on free tier hosts (512MB RAM)
ENV CARGO_BUILD_JOBS=1
ENV RUSTFLAGS="-C link-arg=-Wl,--no-keep-memory"
RUN cargo build --release 2>/dev/null || true

# Build the real binary with actual source
RUN rm -rf src/
COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/vivekarouter .
COPY config.toml* ./
EXPOSE 3001
CMD ["./vivekarouter"]
