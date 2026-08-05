.PHONY: build install test test-format test-cli coverage coverage-html lint fmt bless

# Build the release binary.
build:
	cargo build --release

# Install passdown onto PATH (~/.cargo/bin).
install:
	cargo install --path .

# Run everything: unit + CLI + fixture harness.
test:
	cargo test

# Fixture harness only (tests/fixtures/*).
test-format:
	cargo test --bin passdown format::fixture_tests::fixtures

# End-to-end binary tests (exit codes, config handling).
test-cli:
	cargo test --test cli

# Line and region coverage across all tests, printed per file.
coverage:
	cargo llvm-cov --all-targets

# Same, rendered as an annotated HTML report.
coverage-html:
	cargo llvm-cov --all-targets --open

# Clippy across all targets; warnings fail, locally and in CI alike.
lint:
	cargo clippy --all-targets -- --deny warnings

# Format the Rust source.
fmt:
	cargo fmt

# Rewrite tests/fixtures/*/expected.md (and .diags) from current output.
# Review the diff afterwards -- blessing accepts whatever the code produces.
bless:
	PASSDOWN_BLESS=1 cargo test
