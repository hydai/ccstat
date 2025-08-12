#!/bin/bash
# Build release packages for all platforms

set -e

# Version from Cargo.toml
VERSION=$(grep "^version" Cargo.toml | head -1 | cut -d'"' -f2)
RELEASE_DIR="releases/v$VERSION"

echo "Building ccstat v$VERSION release packages..."

# Create release directory
mkdir -p "$RELEASE_DIR"

# Function to build for a target
build_target() {
    local target=$1
    local binary_name=$2
    local archive_name=$3

    echo "Building for $target..."

    # Add target if not already added
    rustup target add "$target" 2>/dev/null || true

    # Build
    cargo build --release --target "$target"

    # Create archive directory
    local archive_dir="$RELEASE_DIR/ccstat-$VERSION-$archive_name"
    mkdir -p "$archive_dir"

    # Copy binary
    cp "target/$target/release/$binary_name" "$archive_dir/"

    # Copy documentation
    cp README.md LICENSE CHANGELOG.md "$archive_dir/"

    # Create usage examples
    mkdir -p "$archive_dir/examples"
    cp examples/*.rs "$archive_dir/examples/" 2>/dev/null || true

    # Create archive
    if [[ "$archive_name" == *"windows"* ]]; then
        # Windows ZIP
        (cd "$RELEASE_DIR" && zip -r "ccstat-$VERSION-$archive_name.zip" "ccstat-$VERSION-$archive_name")
    else
        # Unix tar.gz
        (cd "$RELEASE_DIR" && tar czf "ccstat-$VERSION-$archive_name.tar.gz" "ccstat-$VERSION-$archive_name")
    fi

    # Cleanup
    rm -rf "$archive_dir"

    echo "Created $archive_name archive"
}

# Build for each platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS builds
    build_target "x86_64-apple-darwin" "ccstat" "macos-x86_64"
    build_target "aarch64-apple-darwin" "ccstat" "macos-aarch64"

    # Create universal binary
    echo "Creating macOS universal binary..."
    mkdir -p "$RELEASE_DIR/ccstat-$VERSION-macos-universal"
    lipo -create \
        "target/x86_64-apple-darwin/release/ccstat" \
        "target/aarch64-apple-darwin/release/ccstat" \
        -output "$RELEASE_DIR/ccstat-$VERSION-macos-universal/ccstat"
    cp README.md LICENSE CHANGELOG.md "$RELEASE_DIR/ccstat-$VERSION-macos-universal/"
    (cd "$RELEASE_DIR" && tar czf "ccstat-$VERSION-macos-universal.tar.gz" "ccstat-$VERSION-macos-universal")
    rm -rf "$RELEASE_DIR/ccstat-$VERSION-macos-universal"

elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux builds
    build_target "x86_64-unknown-linux-gnu" "ccstat" "linux-x86_64"

    # Try to build musl version for better compatibility
    if rustup target add x86_64-unknown-linux-musl 2>/dev/null; then
        build_target "x86_64-unknown-linux-musl" "ccstat" "linux-x86_64-musl"
    fi

    # ARM64 if cross-compilation is set up
    if command -v aarch64-linux-gnu-gcc &> /dev/null; then
        build_target "aarch64-unknown-linux-gnu" "ccstat" "linux-aarch64"
    fi
else
    echo "Unsupported build platform: $OSTYPE"
    echo "Use CI/CD for cross-platform builds"
fi

# Generate checksums
echo "Generating checksums..."
(cd "$RELEASE_DIR" && shasum -a 256 *.tar.gz *.zip 2>/dev/null > checksums.txt || sha256sum *.tar.gz *.zip > checksums.txt)

# Build Docker image
if command -v docker &> /dev/null; then
    echo "Building Docker image..."
    docker build -t ccstat:$VERSION -t ccstat:latest .

    # Save Docker image
    docker save ccstat:$VERSION | gzip > "$RELEASE_DIR/ccstat-$VERSION-docker.tar.gz"
fi

# Create release notes
cat > "$RELEASE_DIR/RELEASE_NOTES.md" <<EOF
# ccstat v$VERSION

## Installation

### macOS
\`\`\`bash
# Intel
curl -L https://github.com/yourusername/ccstat/releases/download/v$VERSION/ccstat-$VERSION-macos-x86_64.tar.gz | tar xz
sudo mv ccstat-$VERSION-macos-x86_64/ccstat /usr/local/bin/

# Apple Silicon
curl -L https://github.com/yourusername/ccstat/releases/download/v$VERSION/ccstat-$VERSION-macos-aarch64.tar.gz | tar xz
sudo mv ccstat-$VERSION-macos-aarch64/ccstat /usr/local/bin/

# Universal
curl -L https://github.com/yourusername/ccstat/releases/download/v$VERSION/ccstat-$VERSION-macos-universal.tar.gz | tar xz
sudo mv ccstat-$VERSION-macos-universal/ccstat /usr/local/bin/
\`\`\`

### Linux
\`\`\`bash
# x86_64
curl -L https://github.com/yourusername/ccstat/releases/download/v$VERSION/ccstat-$VERSION-linux-x86_64.tar.gz | tar xz
sudo mv ccstat-$VERSION-linux-x86_64/ccstat /usr/local/bin/

# x86_64 (static)
curl -L https://github.com/yourusername/ccstat/releases/download/v$VERSION/ccstat-$VERSION-linux-x86_64-musl.tar.gz | tar xz
sudo mv ccstat-$VERSION-linux-x86_64-musl/ccstat /usr/local/bin/
\`\`\`

### Docker
\`\`\`bash
docker load < ccstat-$VERSION-docker.tar.gz
docker run -v ~/.claude:/data ccstat:$VERSION daily
\`\`\`

## Changelog

See CHANGELOG.md for details.
EOF

echo
echo "Release packages created in $RELEASE_DIR:"
ls -la "$RELEASE_DIR"
echo
echo "Next steps:"
echo "1. Test the binaries on their target platforms"
echo "2. Create a GitHub release and upload the archives"
echo "3. Update the download links in the documentation"
