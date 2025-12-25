# Build stage
FROM rust:1.75-bookworm as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /usr/src/rustpush

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY cloudkit-derive/Cargo.toml ./cloudkit-derive/
COPY cloudkit-proto/Cargo.toml ./cloudkit-proto/
COPY open-absinthe/Cargo.toml ./open-absinthe/

# Copy source code and dependencies
COPY . .

# Initialize submodules
RUN git config --global --add safe.directory /usr/src/rustpush && \
    git submodule update --init --recursive

# Build the application
RUN cargo build --release --features macos-validation-data --bin immich-sync

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 rustpush

# Create necessary directories
RUN mkdir -p /app/config /app/data && \
    chown -R rustpush:rustpush /app

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /usr/src/rustpush/target/release/immich-sync /usr/local/bin/immich-sync

# Copy example config
COPY --from=builder /usr/src/rustpush/immich-config.example.plist /app/immich-config.example.plist

# Switch to app user
USER rustpush

# Set environment variables
ENV RUST_LOG=info

# Volume for configuration and state
VOLUME ["/app/config"]

# Set working directory to config
WORKDIR /app/config

# Health check
HEALTHCHECK --interval=60s --timeout=10s --start-period=30s --retries=3 \
    CMD pgrep -f immich-sync || exit 1

# Run the application
CMD ["immich-sync"]
