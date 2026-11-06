# ipsa-contracts

ipsa's on-chain code: the Soroban contracts that make Stellar an IBC v2 chain. They hold the router, the ICS-20 transfer app, the ICS-27 general message passing app, and the light clients through which Stellar verifies its counterparties. The repository root is the Cargo workspace, built to wasm with the Soroban `contract` profile.

| Path | Crate | Role |
|---|---|---|
| `ibc-router/` | `stellar-ibc-router` | IBC v2 router: clients, counterparties, packet routing |
| `ibc-transfer/` | `stellar-ibc-transfer` | ICS-20 fungible-token transfer app |
| `ibc-voucher-token/` | `stellar-ibc-voucher-token` | fungible token the transfer app deploys and mints for unregistered foreign denominations |
| `ibc-gmp/` | `stellar-ibc-gmp` | ICS-27 general message passing app |
| `ibc-fast-fill/` | `stellar-ibc-fast-fill` | fast path for transfers from slow-finality chains: providers pay the recipient before the packet lands, and the packet repays them |
| `gmp-account/` | `ipsa-gmp-account` | account the message passing app derives and deploys per (client, sender, salt), and calls through |
| `testing/light-client/` | `ipsa-test-light-client` | always-accept light client for the tests only; a library with no `cdylib`, so it is never built to wasm or released |
| `light-clients/tendermint/` | `stellar-tendermint-light-client` | Tendermint light client: signed-header verification and ICS-23 proofs |
| `light-clients/sp1-ethereum/` | `stellar-sp1-ethereum-light-client` | Ethereum light client: SP1 proofs of the sync committee and of the router's storage, client type `sepolia` |
| `sp1-verifier/` | `stellar-sp1-verifier` | Groth16 verification of SP1 proofs on BN254 |
| `tests/` | `soroban-contracts-tests` | cross-contract integration tests (end to end transfer, Tendermint header and ICS-23 vectors) |

Every contract crate is `crate-type = ["lib", "cdylib"]`, so each works both as
a Rust dependency in cross-contract tests and as a deployable wasm contract.

## Build

```sh
stellar contract build --profile contract
```

Artifacts land under `target/wasm32v1-none/contract/`.

The router, the transfer app and each light client have their own README with
the contract's API and storage layout.

## Audit

`just audit` runs `cargo audit` with one advisory set aside in the `justfile`'s `ignored_advisories`: RUSTSEC-2026-0387, timing variability in `crypto-bigint` 0.5. It reaches the workspace only through the Soroban SDK's host (`soroban-env-host`, `k256`, `elliptic-curve`), and the fix is in `crypto-bigint` 0.7, which the SDK has not adopted yet. Drop the exception when it does.

## License

Copyright 2026 Amanda Gonsalves. Licensed under the [Apache License 2.0](LICENSE); see also [NOTICE](NOTICE).
