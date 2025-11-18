# Guisu Development Tasks

# List available commands
default:
    @just --list

# Run clippy with pedantic lints
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run tests
test:
    cargo test --workspace

# Analyze binary size (requires: cargo install cargo-bloat)
bloat:
    cargo bloat --release -n 10
    cargo bloat --release --crates

# Check for unused dependencies (requires: cargo install cargo-udeps --locked)
udeps:
    cargo +nightly udeps --workspace

# Build release binary
build:
    cargo build --release
    @ls -lh target/release/guisu | awk '{print $$5, $$9}'

# Clean build artifacts
clean:
    cargo clean

# Format code
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Run cargo check
check:
    cargo check --workspace --all-targets --all-features
