# `ipsa-test-light-client` — a light client for the contracts' tests

Accepts every membership and non-membership proof, reports a timestamp of 0 at every height, and keeps client and consensus state, heights and a frozen flag so the router's client lifecycle can be exercised.

It exists only for tests: the crate is a library with no `cdylib`, so `stellar contract build` never produces a wasm for it, no release carries it, and it cannot be deployed. The router, transfer app and GMP app take it as a dev-dependency.
