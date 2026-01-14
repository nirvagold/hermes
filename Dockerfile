# Multi-stage build for optimal image size
FROM rust:1.83-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY benches ./benches
COPY examples ./examples
COPY tests ./tests

# Build release binaries and examples
RUN cargo build --release --bins --examples

# Runtime stage - minimal image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /build/target/release/hermes_server /app/
COPY --from=builder /build/target/release/hermes_subscriber /app/
COPY --from=builder /build/target/release/examples/battle_test /app/

# Copy scripts
COPY scripts/linux_tuning.sh /app/scripts/

# Create data directory for mmap files
RUN mkdir -p /app/data

# Expose server port
EXPOSE 9999

# Default command
CMD ["/app/hermes_server"]
