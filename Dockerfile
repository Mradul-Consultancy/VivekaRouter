FROM rust:1.78-slim AS builder
WORKDIR /app

# Cache dependencies (dummy source for layer caching)
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs && echo '' > src/lib.rs
# Limit parallelism to reduce memory usage on free tier hosts
ENV CARGO_BUILD_JOBS=2
RUN cargo build --release 2>/dev/null || true

# Build the real binary with actual source
RUN rm -rf src/
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/vivekarouter .
COPY config.toml* ./
EXPOSE 3001
CMD ["./vivekarouter"]
