# Run all checks (mirrors CI)
ci: fmt-check clippy test

# Check formatting without modifying files
fmt-check:
    cargo fmt --check

# Apply formatting
fmt:
    cargo fmt

# Run Clippy lints
clippy:
    cargo clippy -- -D warnings

# Run tests
test:
    cargo test

# Build all targets
build:
    cargo build --all-targets
