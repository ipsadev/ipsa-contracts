# `stellar-ibc-fast-fill` — fast path for transfers from slow-finality chains

A transfer from Sepolia waits about 13 minutes for Ethereum finality before the Stellar light client can accept it. This contract lets a liquidity provider pay the recipient on Stellar within a minute of the Sepolia send, from the provider's own vouchers. When the proven IBC packet lands about 20 minutes later, its vouchers go to the provider instead of the recipient. No token is minted or released except by a proven packet; the provider carries the timing and reorg risk.

The contract is the Stellar half. The Sepolia half is a fast-transfer contract in `ibc-contracts` that sends the packet, receives refunds through `ICS20Transfer`'s sender callbacks, and pays them to the provider named in a refund notice.

## The order

The fast-transfer contract on Sepolia sends an ordinary ICS-20 packet whose receiver is this contract, and whose memo names this contract as the destination callback and carries the fill terms:

```json
{"dest_callback":{"address":"C…FASTFILL"},"fast_fill":"G…RECIPIENT:FEE:DEADLINE"}
```

- `RECIPIENT` is the Stellar address that receives the tokens.
- `FEE` is what the provider keeps, in the token's base units; the recipient gets `amount − FEE` at fill time.
- `DEADLINE` is a Unix timestamp after which the order can no longer be filled. It must fall well before the packet's timeout.

An order is identified by a key both chains compute the same way:

```text
key = sha256(uint64 sequence ‖ uint256 amount ‖ sha256(source client) ‖ sha256(denom) ‖ sha256(memo))
```

with the integers big-endian, which is Solidity's `abi.encodePacked(uint64, uint256, bytes32, bytes32, bytes32)`. `source client` is the client id on the sending chain, `denom` the packet's denomination as sent, and `memo` the packet memo byte for byte. A fill that misstates any of them produces a key no packet will ever match, so its provider is never repaid and the real order stays open.

## Entry points

- `fill(provider, order, provider_evm)`: the provider pays `amount − fee` to the recipient and the fill is recorded under the order key. It is refused when the order is already filled or delivered, the deadline has passed, the fee is not below the amount, the memo does not address this contract, `provider_evm` is not a 20-byte hex address, or the token has no route.
- `on_ibc_transfer_received(transfer_app, received)`: the transfer app's destination callback. Only the configured transfer app may call it. A matching fill sends the packet's full amount to the provider; otherwise it goes to the recipient in the memo. Either way the order is marked settled, so it can no longer be filled. A packet with no valid fill terms panics, so the router acknowledges it with an error and Sepolia refunds the sender.
- `notify_refund(key)`: callable by anyone for a filled order that has not been delivered. It sends one ICS-27 call through the GMP app to the packet's sender on the source chain, `assignRefund(bytes32 key, address provider)`, so a refund from a failed or timed-out packet is paid to the provider rather than the user. A notice for a packet that later succeeds is harmless: no refund ever arrives.
- `order_key(order)`, `fill_of(key)`, `is_settled(key)`, `route(dest_client, denom)`, `config()`: read-only.

## Owner configuration

- `set_route(dest_client, denom, token)` and `remove_route(dest_client, denom)` list the tokens that can be filled: the packet's denomination as it arrives over `dest_client`, and the Stellar token the transfer app credits for it. The route ties the token to the packet, so a provider cannot fill with a different token. The route table is also the list of assets the fast path serves.
- `configure(config)` sets the transfer app, the GMP app, the encoding of refund notices (`application/x-solidity-abi` for EVM chains) and how long a notice packet may take before it times out.
- `upgrade(wasm_hash)`.

## Tests

`tests/fill.rs` covers filling and delivery, `tests/refund.rs` the refund notice and its calldata, and `tests/order_key.rs` checks the key against the packed encoding computed independently. They run against the real router and transfer app with an always-accept light client and a recording GMP contract (`tests/common/mod.rs`).
