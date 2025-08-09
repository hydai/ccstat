# Multi-stage Dockerfile for ccstat
# Builds a minimal container with just the ccstat binary

# Build stage
FROM rust:1.89-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static

# Create app directory
WORKDIR /app

# Copy all project files (dockerignore will exclude unwanted files)
COPY . .

# Build release binary (only the binary, skip building examples and benches)
RUN cargo build --release --locked --bin ccstat

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    openssl \
    tini

# Create non-root user
RUN addgroup -g 1000 ccstat && \
    adduser -D -u 1000 -G ccstat ccstat

# Copy binary from builder
COPY --from=builder /app/target/release/ccstat /usr/local/bin/ccstat

# Set up data directory
RUN mkdir -p /data && chown ccstat:ccstat /data
VOLUME ["/data"]

# Switch to non-root user
USER ccstat

# Set environment variable for data path
ENV CLAUDE_DATA_PATH=/data

# Use tini as entrypoint to handle signals properly
ENTRYPOINT ["/sbin/tini", "--"]

# Default command
CMD ["ccstat", "--help"]
