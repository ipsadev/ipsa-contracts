# Soroban contracts: the IBC router, the ICS-20 and GMP apps, and the light clients

lints := "-D warnings -A clippy::manual_is_multiple_of -A clippy::too_many_arguments -A clippy::result_large_err"

ignored_advisories := "--ignore RUSTSEC-2026-0104 --ignore RUSTSEC-2026-0098 --ignore RUSTSEC-2026-0099 --ignore RUSTSEC-2026-0049 --ignore RUSTSEC-2026-0009 --ignore RUSTSEC-2026-0258"

default:
    @just --list

# everything CI runs, in CI's order
ci: fmt-check lint test audit

# the contract wasm that tests import through contractimport!
build:
    stellar contract build --profile contract

fmt:
    cargo +nightly fmt --all

fmt-check:
    cargo fmt --all -- --check

lint: build
    cargo clippy --locked --all-targets -- {{lints}}

test: build
    cargo test --locked

audit:
    cargo audit --file Cargo.lock {{ignored_advisories}}

clean:
    cargo clean
