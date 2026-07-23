.DEFAULT_GOAL := check

.PHONY: check lint test-rust test-python docs-benchmark docs-prepare docs docs-serve coverage

check: lint test-rust test-python docs

lint:
	cargo fmt --all -- --check
	cargo fmt --manifest-path fuzz/Cargo.toml -- --check
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
	cargo clippy --manifest-path fuzz/Cargo.toml --bins --locked -- -D warnings
	uv run --locked ruff check .
	uv run --locked ruff format --check .
	uv run --locked ty check

test-rust:
	cargo test --workspace --all-features --locked

test-python:
	uv --directory crates/h11r-python run --locked maturin develop --release
	uv run --locked pytest

# Keep the public benchmark summaries derived from the recorded pyperf result.
docs-benchmark:
	uv run --locked --group benchmark python \
		crates/h11r-python/benchmarks/render_results.py \
		docs/assets/python-benchmark.json \
		--svg docs/assets/python-benchmark.svg \
		--table target/docs/python-benchmark.md

# mkdocstrings inspects the compiled PyO3 module, so prepare it before any
# Zensical or mike build.
docs-prepare: docs-benchmark
	uv run --project "$(CURDIR)" --directory crates/h11r-python \
		--group docs --locked maturin develop --release

docs: docs-prepare
	uv run --group docs --locked zensical build --strict

docs-serve: docs-prepare
	uv run --group docs --locked zensical serve

# Use one LLVM environment for Cargo tests and the PyO3 extension build so the
# Rust report includes both execution paths.
coverage:
	set -e; \
	eval "$$(cargo llvm-cov show-env --sh)"; \
	cargo llvm-cov clean --workspace; \
	cargo test --workspace --all-features --locked; \
	uv --directory crates/h11r-python run --locked maturin develop; \
	uv run --locked coverage run -m pytest; \
	uv run --locked coverage xml; \
	cargo llvm-cov report --lcov --output-path rust.lcov
