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
| `light-clients/tendermint/` | `stellar-tendermint-light-client` | Tendermint light client: signed-header verification, with ICS-23 proofs from `ipsa-ics23` |
| `crates/ics23/` | `ipsa-ics23` | ICS-23 membership and non-membership proofs for a Cosmos SDK chain, one key per proof level; a library, not a contract |
| `light-clients/sp1-ethereum/` | `stellar-sp1-ethereum-light-client` | Ethereum light client: SP1 proofs of the sync committee and of the router's storage, client type `sepolia` |
| `crates/sp1-verifier/` | `stellar-sp1-verifier` | Groth16 verification of SP1 proofs on BN254 |
| `tests/` | `soroban-contracts-tests` | cross-contract integration tests, one file per area under `tests/tests/` (end to end transfer, Tendermint headers, and ICS-23 proofs through the router and the Tendermint client) |

Every contract crate is `crate-type = ["lib", "cdylib"]`, so each works both as
a Rust dependency in cross-contract tests and as a deployable wasm contract.

## Build

```sh
stellar contract build --profile contract
```

Artifacts land under `target/wasm32v1-none/contract/`.

The router, the transfer app and each light client have their own README with
the contract's API and storage layout.

## Release

A release builds every contract to wasm, writes `SHA256SUMS`, and compares each hash against the previous release. A soroban wasm hash is the sha256 of the wasm file, so the comparison needs no new artifact: the hash a contract would be upgraded to is the line already in `SHA256SUMS`. The hash moves on any change under `src/`, formatting included, because panic locations are compiled in, so a changed contract means different bytes, not necessarily different behaviour.

A release does not redeploy the network. A router holds the clients, counterparties, commitments and vouchers, so a fresh router is a fresh network that nothing points at. Between releases the live router is upgraded in place with `upgrade(wasm_hash)` and clients are moved onto new light client code with `migrate_client`, and the release supports that:

- The router, the transfer app and the gmp app are never deployed by a release. Their wasm hash is published and compared, so an operator knows whether to run `ipsa tx contracts upgrade --wasm_hash <hash>` on the live contract.
- A light client whose hash changed is deployed, with the live router as its admin (the router address comes from the `ROUTER_ADDRESS` variable on the `testnet` environment, not a fresh deploy), and the release then migrates its corridor onto it in place: it reads the current client and consensus state from the old light client, calls the router's `migrate_client`, and re-registers the client type (`07-tendermint` or `sepolia`) at the new light client so clients created afterward also run the new code. The corridor each client serves comes from the `TENDERMINT_CLIENT_ID` and `SEPOLIA_CLIENT_ID` variables on the `testnet` environment; a changed client with no corridor id set fails the release rather than deploying an orphan. The router owner signs the migration and the re-registration, so the release's signing key must own the router.
- A light client whose hash is unchanged is not deployed.
- A fresh full set is deployed and wired only when the workflow is run with the `deploy_router` input, which keeps the first release's full deploy. This is how the first release bootstraps the network, since the default path needs a live router to migrate onto; a plain tag push always takes the default path.

### Setting up a new network

A `deploy_router` run writes the new router, app and light client addresses to `deployments.json`, but it cannot update the workflow's own environment. After one, set the `testnet` environment's variables to the new network before the next default release: `ROUTER_ADDRESS` to the new router, and `TENDERMINT_CLIENT_ID` / `SEPOLIA_CLIENT_ID` to the corridor client ids once the gateway has created them (a fresh deploy registers the client types but creates no clients). A default release run before these are set fails rather than migrating onto the old network.

The release notes carry a table of every contract: its wasm hash, the previous release's hash, whether it changed, and what to do when it did. They also carry a contract addresses table. The light client addresses are resolved live from the router, and the router's own address is `ROUTER_ADDRESS`; the transfer, gmp and fast-fill apps are not deployed by a release, so their addresses are read from the optional `TRANSFER_ADDRESS`, `GMP_ADDRESS` and `FAST_FILL_ADDRESS` variables on the `testnet` environment, and a contract with no address is left out of the table. `deployments.json` records the same hashes, the changed flags and any deployed light client address, so tooling can read them. The first release has no previous release to compare against, so every contract counts as changed.

A dry run (`workflow_dispatch` with `dry_run`) runs the checks, builds the wasm, and prints the table, without deploying or publishing.

### Rollout order after a release

The release already migrates each changed light client's corridor onto the new code, so the remaining steps are the in-place upgrades the release cannot do. Apply them to staging in this order:

1. Upgrade the live router if its hash changed: `ipsa tx contracts upgrade --wasm_hash <router hash>`.
2. Upgrade the transfer and gmp apps if their hashes changed, the same way.
3. Set a new voucher or account wasm on its app if its hash changed (`set_voucher_wasm` / `set_account_wasm --wasm_hash <hash>`).
4. Apply any counterparty prefix or client type changes last.

The client migration sits with the release itself: it runs when the release deploys a changed light client, reading the corridor's current client and consensus state and re-initialising the new client from it. A release that has already published is never re-run; one that failed before publishing deploys a fresh client and migrates again from the corridor's current state, which is safe.

## Audit

`just audit` runs `cargo audit` with one advisory set aside in the `justfile`'s `ignored_advisories`: RUSTSEC-2026-0387, timing variability in `crypto-bigint` 0.5. It reaches the workspace only through the Soroban SDK's host (`soroban-env-host`, `k256`, `elliptic-curve`), and the fix is in `crypto-bigint` 0.7, which the SDK has not adopted yet. Drop the exception when it does.

## License

Copyright 2026 Amanda Gonsalves. Licensed under the [Apache License 2.0](LICENSE); see also [NOTICE](NOTICE).
