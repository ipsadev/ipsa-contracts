# `stellar-ibc-gmp` — ICS-27 general message passing app

The `gmpport` application on the router. It sends calls from Stellar to contracts on other IBC v2 chains, and runs calls from other chains on Stellar.

## Encodings

Each packet names its encoding, and the app reads and writes three:

- `application/x-solidity-abi`, the only one Ethereum's `ICS27GMP` accepts. The packet is `abi.encode(GMPPacketData{string sender; string receiver; bytes salt; bytes payload; string memo})` and the acknowledgement `abi.encode(GMPAcknowledgement{bytes result})`, byte for byte as Solidity produces them (`tests/abi.rs` checks against Foundry's encoder).
- `application/x-soroban-xdr`, the XDR of the same fields, between Soroban contracts.
- `application/json`, for Cosmos chains.

A call is acknowledged in the encoding it arrived in. To call a contract on Ethereum from Stellar, pass `application/x-solidity-abi` to `send_call`, the `0x` address as `receiver` and the Solidity calldata as `payload`. A call from Ethereum to Stellar carries, as its `payload`, the XDR of a `GmpCall` (target contract, function and arguments).

## Receiving a call

Each call runs from an account the app derives from the destination client, the sender and the salt (`gmp-account`), deploying it on first use. The account calls the target contract and the result is returned in the acknowledgement. A call that fails is acknowledged with IBC v2's universal error acknowledgement, through the router, so the sender learns it failed.

## Learning the result of a call

`send_call` takes the sending address. When the sender is a contract, the app reports the outcome back to it when the acknowledgement or timeout returns:

```rust
fn on_gmp_acknowledgement(env: Env, gmp: Address, result: GmpAcknowledged);

fn on_gmp_timeout(env: Env, gmp: Address, result: GmpTimedOut);
```

`GmpAcknowledged` carries the source and destination clients, the sequence, `success` (false for the universal error acknowledgement) and the raw acknowledgement, which on success holds the destination call's result. `GmpTimedOut` carries the clients and the sequence. A contract should check that `gmp` is the app it trusts and call `gmp.require_auth()`, which succeeds only when the GMP app is the caller.

The calls are made with `try_invoke_contract` and their failures are ignored, so a missing or failing callback never blocks an acknowledgement or a timeout. Account senders are not called back.

## Upgrading

`upgrade(wasm_hash)` is owner-only and replaces the app's code in place, keeping its address, its storage and the router's `gmpport` registration.
