# Smart Contracts

The bridge's on-chain code: the Stellar Soroban contracts behind IBC v2 — the
router, the ICS-20 transfer app, the ICS-27 general message passing app, and
the light clients. The repository root is the Cargo workspace, built to wasm
with the Soroban `contract` profile.

| Path | Crate | Role |
|---|---|---|
| `ibc-router/` | `stellar-ibc-router` | IBC v2 router: clients, counterparties, packet routing |
| `ibc-transfer/` | `stellar-ibc-transfer` | ICS-20 fungible-token transfer app |
| `ibc-voucher-token/` | `stellar-ibc-voucher-token` | fungible token the transfer app deploys and mints for unregistered foreign denominations |
| `ibc-gmp/` | `stellar-ibc-gmp` | ICS-27 general message passing app |
| `gmp-account/` | `ipsa-gmp-account` | account the message passing app derives and deploys per (client, sender, salt), and calls through |
| `light-clients/mock/` | `stellar-mock-light-client` | always-accept light client (development) |
| `light-clients/attestation/` | `stellar-attestation-light-client` | m-of-n Ed25519 attestation light client |
| `light-clients/tendermint/` | `stellar-tendermint-light-client` | Tendermint light client: signed-header verification and ICS-23 proofs |
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

## License

Copyright 2026 Amanda Gonsalves. Licensed under the [Apache License 2.0](LICENSE).
