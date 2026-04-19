.PHONY: test coverage coverage-html coverage-summary clean-coverage

# Run the full test suite serialised. `#[serial]` guards already serialise
# anything that touches process-global state (env, CWD), but we pin
# --test-threads=1 as belt-and-braces for the coverage runs too.
test:
	cargo test --workspace -- --test-threads=1

# Emit lcov.info at the repo root for CI upload / external tools.
coverage:
	cargo llvm-cov --workspace --lcov --output-path lcov.info -- --test-threads=1

# Human-browsable HTML report at target/coverage/html/index.html.
coverage-html:
	cargo llvm-cov --workspace --html --output-dir target/coverage -- --test-threads=1

# Quick line/region/function percentages, no files written.
coverage-summary:
	cargo llvm-cov --workspace --summary-only -- --test-threads=1

clean-coverage:
	cargo llvm-cov clean --workspace
	rm -f lcov.info
	rm -rf target/coverage
