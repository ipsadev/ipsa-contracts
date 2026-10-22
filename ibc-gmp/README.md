# `stellar-ibc-gmp` — ICS-27 general message passing app

The `gmpport` application on the router, version `ics27-2`. It sends calls from Stellar to contracts on other IBC v2 chains, and runs calls from other chains on Stellar. It is the same protocol as ibc-go's `27-gmp` module and Solidity's `ICS27GMP`.

## Encodings

Each packet names its encoding, and the app reads and writes four. A call is acknowledged in the encoding it arrived in.

| Encoding | Counterparty | Packet | Acknowledgement |
|---|---|---|---|
| `application/x-solidity-abi` | Solidity `ICS27GMP`, which accepts only this | `abi.encode(GMPPacketData{string sender; string receiver; bytes salt; bytes payload; string memo})` | `abi.encode(GMPAcknowledgement{bytes result})` |
| `application/x-protobuf` | ibc-go `27-gmp` | `GMPPacketData` fields 1 to 5 | `Acknowledgement{bytes result = 1}` |
| `application/json` | ibc-go `27-gmp` | `{"sender","receiver","salt","payload","memo"}`, bytes in base64 | `{"result":"<base64>"}` |
| `application/x-soroban-xdr` | another Soroban router | the XDR of `GmpPacketData` | the XDR of `Acknowledgement{result}` |

ibc-go refuses an acknowledgement that does not re-encode to the same bytes, so the protobuf and JSON forms follow its encoder exactly. `tests/ibc_go_vectors.rs` checks packets and acknowledgements against bytes produced by ibc-go v11's own `MarshalPacketData` and `MarshalAcknowledgement`, and `tests/abi.rs` checks the ABI form against Foundry's encoder. A protobuf packet with an unknown field is refused, as ibc-go refuses it.

To call a contract on Ethereum from Stellar, pass `application/x-solidity-abi` to `send_call`, the `0x` address as `receiver` and the Solidity calldata as `payload`.

## Sending a call

```rust
fn send_call(
    env: Env,
    sender: Address,
    source_client_id: String,
    receiver: String,
    salt: Bytes,
    payload: Bytes,
    timeout_timestamp: u64,
    memo: String,
    encoding: String,
) -> u64;
```

`sender` must authorise the call. The call is identified by `(source_client_id, sequence)`, the sequence being the return value. Limits: sender and receiver 256 bytes, salt 64, payload 32,768, memo 4,096.

## Receiving a call

A call to Stellar carries, as its `payload`, the XDR of a `GmpCall`: the target contract, the function and its arguments.

- **Receiver.** `receiver` must be the target contract's address, or empty. A packet that names one contract and calls another is refused (`ReceiverMismatch`), so what an explorer shows as the receiver is what ran.
- **The remote sender's account.** Each call runs from an account the app derives from the local client the packet arrived on, the sender and the salt (`gmp-account`), deploying it on first use. The account calls the target with exactly the given function and arguments and nothing else.
- **What the target sees.** The target is called by that account, with no extra arguments. It authenticates the remote sender by checking the account:

```rust
pub fn set_limit(env: Env, limit: u32) -> u32 {
    let remote_account: Address = env.storage().instance().get(&REMOTE_ACCOUNT).unwrap();

    remote_account.require_auth();

    ...
}
```

  Store the account address when the contract is set up. Get it from `account_address(client_id, sender, salt)` on the GMP app, which returns the address whether or not the account exists yet. A target cannot ask the GMP app during the call, because Soroban does not let a contract be re-entered while it is running, and the GMP app is still running when the target executes. `tests/router_receive.rs` shows the pattern end to end through the router.
- **The result.** The value the target returns is acknowledged as its XDR, inside the acknowledgement for the packet's encoding. A call that fails, including one refused by the target's `require_auth`, is acknowledged with IBC v2's universal error acknowledgement.

This is not ICS-27 v1 interchain accounts: there are no channels or ordering, and there is one account per client, sender and salt.

## Learning the result of a call

When the sender is a contract, the app reports the outcome back to it once the acknowledgement or timeout returns:

```rust
fn on_gmp_acknowledgement(env: Env, gmp: Address, result: GmpAcknowledged);

fn on_gmp_timeout(env: Env, gmp: Address, result: GmpTimedOut);
```

- `GmpAcknowledged` carries the source and destination clients, the sequence, `success` (false for the universal error acknowledgement), `result` and the raw `acknowledgement`.
- `result` is the destination call's result, taken out of the acknowledgement in the packet's encoding: the XDR of the returned value for a Soroban target, the return data for a Solidity target. It is empty on failure, or when the acknowledgement cannot be read; the raw acknowledgement is always there.
- `GmpTimedOut` carries the clients and the sequence.
- A contract should check that `gmp` is the app it trusts and call `gmp.require_auth()`, which succeeds only when the GMP app is the caller.

The calls are made with `try_invoke_contract` and their failures are ignored, so a missing or failing callback never blocks an acknowledgement or a timeout. Account senders are not called back. A contract built against the `GmpAcknowledged` type before `result` was added no longer decodes the callback, and is silently not called back until it is rebuilt.

## Upgrading

`upgrade(wasm_hash)` is owner-only and replaces the app's code in place, keeping its address, its storage and the router's `gmpport` registration.
