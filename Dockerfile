# Multi-stage build for smaller final image
FROM rust:1.83-slim AS builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy the actual source code
COPY src ./src

# Build the application and seeder
# Touch main.rs to ensure it's rebuilt
RUN touch src/main.rs && \
    cargo build --release && \
    cargo build --release --bin seed

# Runtime stage
FROM debian:bookworm-slim

# Install CA certificates for HTTPS requests and wget for health checks
RUN apt-get update && \
    apt-get install -y ca-certificates wget && \
    rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 appuser

WORKDIR /app

# Copy the binaries from builder
COPY --from=builder /app/target/release/currency-rates /usr/local/bin/currency-rates
COPY --from=builder /app/target/release/seed /usr/local/bin/seed-db

# Copy environment file
COPY .env ./

# Copy seed data files (optional - comment out if not using)
COPY seed_data ./seed_data

# Create directory for database and set permissions
RUN mkdir -p /app/data && \
    chown -R appuser:appuser /app

USER appuser

# Expose the API port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -q --spider http://localhost:8080/health || exit 1

# Run the application
CMD ["currency-rates"]
