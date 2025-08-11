# Release Checklist for ccstat

This document outlines the release process for ccstat.

## Pre-Release Checklist

### Code Quality
- [ ] All tests pass: `cargo test`
- [ ] No clippy warnings: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Code is formatted: `cargo fmt`
- [ ] Documentation builds: `cargo doc --no-deps`
- [ ] Benchmarks run: `cargo bench`

### Feature Validation
- [ ] Daily aggregation works correctly
- [ ] Monthly aggregation works correctly
- [ ] Session tracking works correctly
- [ ] Billing blocks calculation is accurate
- [ ] Cost calculations match TypeScript implementation
- [ ] All CLI flags work as documented
- [ ] JSON output format is correct

### Cross-Platform Testing
- [ ] macOS (x86_64)
- [ ] macOS (ARM64)
- [ ] Linux (x86_64)
- [ ] Linux (ARM64)
- [ ] Windows (x86_64)

### Documentation
- [ ] README.md is up to date
- [ ] USAGE.md covers all features
- [ ] ARCHITECTURE.md reflects current design
- [ ] CHANGELOG.md updated with new version
- [ ] API documentation is complete

### Version Updates
- [ ] Cargo.toml version bumped
- [ ] Cargo.lock updated
- [ ] Git tag created: `git tag -a v0.1.0 -m "Release v0.1.0"`

## Release Process

### 1. Final Checks
```bash
# Run all tests
cargo test

# Check formatting
cargo fmt -- --check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Build release binaries
cargo build --release
```

### 2. Test Release Binaries
```bash
# Test basic functionality
./target/release/ccstat daily
./target/release/ccstat monthly
./target/release/ccstat session
./target/release/ccstat blocks
```

### 3. Create Changelog Entry
Update CHANGELOG.md with:
- Version number and date
- New features
- Bug fixes
- Breaking changes
- Performance improvements

### 4. Commit and Tag
```bash
# Commit version bump and changelog
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: bump version to v0.1.0"

# Create annotated tag
git tag -a v0.1.0 -m "Release v0.1.0"

# Push changes and tag
git push origin master
git push origin v0.1.0
```

### 5. Monitor CI/CD
- Check GitHub Actions for successful builds
- Verify all platforms built successfully
- Ensure security audit passes

### 6. Create GitHub Release
The release workflow will create a draft release. Update it with:
- Release notes from CHANGELOG.md
- Installation instructions
- Notable changes
- Contributors

### 7. Publish Release
- Review draft release
- Verify all assets are uploaded
- Publish the release

### 8. Post-Release
- [ ] Announce on relevant channels
- [ ] Update documentation site (if applicable)
- [ ] Monitor for issues
- [ ] Consider publishing to crates.io

## Release Asset Naming

The CI/CD pipeline creates these assets:
- `ccstat-linux-x86_64.tar.gz`
- `ccstat-macos-x86_64.tar.gz`
- `ccstat-macos-aarch64.tar.gz`
- `ccstat-windows-x86_64.exe.zip`

## Rollback Procedure

If issues are discovered post-release:

1. Delete the GitHub release (keep the tag)
2. Fix the issue
3. Create a new patch version (e.g., v0.1.1)
4. Follow the release process again

## Version Numbering

Follow Semantic Versioning (SemVer):
- MAJOR version for incompatible API changes
- MINOR version for backwards-compatible functionality
- PATCH version for backwards-compatible bug fixes

## Crates.io Publishing (Future)

When ready to publish to crates.io:

```bash
# Dry run
cargo publish --dry-run

# Publish
cargo publish
```

Ensure you have:
- Valid crates.io API token
- All dependencies are published
- License is specified in Cargo.toml