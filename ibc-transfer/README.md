# `stellar-transfer-app` — ICS-20 v2 token transfer app

The first real ICS-26 application contract. Stellar analog of Eureka's
`ICS20Transfer.sol` (without the IBCERC20 wrapping yet — `TODO(sac)` markers
mark where Stellar Asset Contract integration plugs in).

Demonstrates the full packet lifecycle by exercising every router entrypoint:
mints sequences via `send_packet`, receives ack/timeout callbacks, refunds on
error.

## What it does

- **Outbound (user → counterparty):**
  - `initiate_transfer(sender, source_client_id, denom, amount, receiver, timeout, memo)`
    auths the sender, enforces the per-denom daily rate limit (Phase D.2),
    debits the sender's local balance, credits the app's own escrow address,
    encodes a `FungibleTokenPacketData` in the encoding configured for
    `source_client_id` (see [Encodings](#encodings)), packages it into a
    `Payload` (port=`transfer`, version=`ics20-1`), and calls
    `router.send_packet` via `env.invoke_contract`. Publishes `TransferSent`
    and returns the minted sequence.
  - A contract can be the sender. The escrow is a token `transfer` from the
    sender to this app, one call below `initiate_transfer`, so the contract
    authorises it first with `authorize_as_current_contract` (see
    `tests/sender_callbacks.rs`).
- **Inbound (counterparty → user):**
  - `on_recv_packet(callback)` is dispatched by the router; the app decodes
    the packet data, resolves the denom through its trace (see
    [Denom trace](#denom-trace)), credits the receiver, publishes
    `TransferCredited`, and returns the success ack `{"result":"AQ=="}`.
  - A packet whose version is not `ics20-1`, or whose amount is zero or
    negative, is refused, and the router answers it with the universal error
    ack.
- **Ack handling:**
  - `on_acknowledgement_packet(callback)` is dispatched by the router after
    the counterparty's ack arrives. If the ack matches the success sentinel
    `[0x01]` or starts with `{"result"` (what ibc-go and upstream's
    `ICS20Transfer.sol` both write), the escrow stays with the counterparty.
    Otherwise — including EVM's universal error ack — refund the sender from
    escrow and publish `TransferRefunded`.
- **Timeout handling:**
  - `on_timeout_packet(callback)` is dispatched by the router when the
    packet times out without delivery; the app refunds the sender from
    escrow and publishes `TransferRefunded`.
- **Telling a contract sender the outcome:**
  - When the sender is a contract, the app calls it after the outcome is
    settled, so any refund has already landed:
    `on_transfer_acknowledgement(transfer: Address, result: TransferAcknowledged)`
    with `success` and the raw acknowledgement, or
    `on_transfer_timeout(transfer: Address, result: TransferTimedOut)`.
  - The sender should check that `transfer` is the app it called and call
    `transfer.require_auth()`.
  - A failing or missing callback is ignored, so a broken sender can never
    block its own refund. Account senders get no callback.

## Who calls it

| Caller | Calls |
|---|---|
| Off-chain admin (deploy) | `__constructor(router, admin)`, `set_rate_limit(denom, daily_cap)`, `set_client_encoding(client_id, encoding)`, `register_asset(denom, address, voucher)` |
| Users (wallets, scripts) | `initiate_transfer(...)`, `asset(denom) -> AssetInfo`, `assets()`, `balance_of(...)`, `daily_cap(...)`, `daily_usage(...)` |
| Tests / dev seeding | `mint(to, denom, amount)` — `TODO(sac)`: replace with SAC `transfer` once integrated |
| The router | `on_recv_packet`, `on_acknowledgement_packet`, `on_timeout_packet` — never called by anyone else (router address pins itself via `require_auth`) |

## Wire shape

```rust
struct Token { denom: String, amount: i128 }

struct FungibleTokenPacketData {
    token: Token,
    sender: String,
    receiver: String,
    memo: String,
}
```

## Encodings

`Payload.value` carries `FungibleTokenPacketData` in one of two encodings, and
the counterparty decides which: a Cosmos chain running ibc-go reads JSON, and
upstream's `ICS20Transfer.sol` accepts **only** `application/x-solidity-abi`
and rejects anything else with the universal error ack.

| `Payload.encoding` | Wire form | Counterparty |
|---|---|---|
| `application/json` (default) | `{"denom","amount","sender","receiver","memo"}`, amount as a decimal string | ibc-go |
| `application/x-solidity-abi` | `abi.encode((string denom, string sender, string receiver, uint256 amount, string memo))` | Solidity `ICS20Transfer` |

**Outbound**, the encoding is chosen per client: `set_client_encoding(client_id,
encoding)` (owner-only, refuses anything not in the table) and
`client_encoding(client_id)`, which returns JSON for an unconfigured client. It
is per client rather than per call because the counterparty chain behind a
client id does not change, and a sender choosing wrongly gets a refund after a
wasted round trip rather than an error.

**Inbound**, and when an ack or timeout needs the original packet to refund
from, the payload is decoded by its own `encoding` field, so one app serves both
kinds of counterparty at once. An unknown encoding panics with
`UnsupportedEncoding` (11).

The ABI codec is checked byte for byte against `cast abi-encode` in
`tests/abi_encoding.rs`, and the ABI path through the router — the committed
packet, a JSON success ack from EVM, EVM's universal error ack and a timeout —
in `tests/solidity_counterparty.rs`. An amount must fit a positive `i128`; the
top half of the `uint256` word is refused rather than truncated.

The denom convention for a Stellar asset sent to EVM is its **SAC contract id**
(`C…`), mirroring upstream's use of the ERC-20 address as the denom for a
native token. `ICS20Transfer` then mints an `IBCERC20` named
`transfer/<its client id for Stellar>/<C…>`.

## Denom trace

Standard ICS-20, the same rule upstream's `ICS20Transfer.sol` applies. On
receive, with the packet's `source_port`/`source_client` and
`dest_port`/`dest_client`:

| Received denom | Meaning | Credited as |
|---|---|---|
| starts with `{source_port}/{source_client}/` | the token is coming home | the rest of the denom — a `Local` asset is released from escrow |
| anything else | the token is foreign | `{dest_port}/{dest_client}/{denom}` — a voucher, deployed on first receipt |

So an ERC-20 sent from Sepolia arrives as `transfer/sepolia-0/0x39a0…`, and
sending that voucher back puts the full trace in the packet, which Sepolia strips
before releasing the ERC-20. A prefix naming another client is foreign, not
returning, and a prefix with nothing after it is refused. Outbound, the denom is
sent exactly as held, so a voucher always leaves with its trace.

Before this, the app credited the received denom verbatim: a foreign token became
a voucher with no trace, which the origin chain would treat as foreign on the way
back, and a returning token minted a new voucher instead of releasing escrow.
`tests/denom_trace.rs` covers the rule; the older receive tests were corrected to
expect traced denoms.

## Upgrading

`upgrade(wasm_hash)` is owner-only and replaces the contract's code in place, the
way upstream's `ICS20Transfer` is a UUPS proxy. It exists because the router's
`register_port` never overwrites: without it, fixing the app meant a new router,
and a new router meant a new light client on the counterparty, whose client state
pins the router's id. The router itself is still not upgradeable.

## Entrypoints

- `__constructor(router, admin)`
- `set_rate_limit(denom, daily_cap)` — admin-only
- `daily_cap(denom) -> Option<i128>` / `daily_usage(denom) -> i128`
- `mint(to, denom, amount)` / `balance_of(who, denom) -> i128`
- `set_client_encoding(client_id, encoding)` — owner-only / `client_encoding(client_id) -> String`
- `set_voucher_wasm(wasm_hash)` / `upgrade(wasm_hash)` — owner-only
- `initiate_transfer(sender, source_client_id, denom, amount, receiver, timeout_timestamp, memo) -> u64`
- `on_recv_packet(callback) -> Bytes`
- `on_acknowledgement_packet(callback)`
- `on_timeout_packet(callback)`

## When to use it

- Demoing the full IBC v2 packet round-trip on Stellar.
- Reference for writing other ICS-26 app contracts — the on_recv/on_ack/
  on_timeout dispatch pattern and the router auth check transfer directly.

**Not production-ready yet.** Two follow-ups:
- SAC integration (`TODO(sac)`) — the local-balance ledger has no link to
  real Stellar assets. Outbound should call SAC `transfer(sender, escrow,
  amount)`; inbound should `transfer(escrow, receiver, amount)` for native
  denoms and mint a wrapped denom (IBCERC20-equivalent) for foreign denoms.
- Per-client escrow isolation — currently single shared
  `current_contract_address` escrow. Eureka's per-client escrows are an
  isolation feature for multi-tenant deployments.

## Outbound flow (Stellar → counterparty)

```
+-----------+   initiate_transfer    +---------------------+
|  sender   |----------------------->|  IbcTransferApp     |
+-----------+ (sender auths)         |   (this crate)      |
                                     +----------+----------+
                            enforce_rate_limit  |
                            debit(sender, denom)|
                            credit(escrow, denom)|
                            commit_v2_packet    |  send_packet (env.invoke_contract)
                                                v
                                     +----------+----------+
                                     |  IbcRouter          |
                                     +----------+----------+
                                                |
                                       set_packet_commitment
                                       emit SendPacket event
                                                |
                                                v
                                  [relayer picks up event,
                                   submits MsgRecvPacket
                                   to counterparty chain]
```

## Inbound flow (counterparty → Stellar)

```
[relayer submits MsgRecvPacket via gateway]
                |
                v
       +--------+--------+
       |  IbcRouter      |
       +--------+--------+
                |  verify_membership (via LC)
                |  set_packet_receipt
                |
                v  on_recv_packet(callback)
       +--------+--------+
       |  IbcTransferApp |
       |   (this crate)  |
       +--------+--------+
                |  decode FungibleTokenPacketData
                |  credit(receiver, denom)
                v
       returns ack = {"result":"AQ=="}
                |
                v
       +--------+--------+
       |  IbcRouter      |
       +--------+--------+
                |  commit_v2_acknowledgement
                |  set_ack_commitment
                |  emit RecvPacket + WriteAck events
                v
    [relayer sees ack event, submits MsgAckPacket
     to source chain; source chain dispatches
     on_acknowledgement_packet — see ack handling
     in this crate above]
```

## Transfer and call

A transfer can deliver tokens to a contract and call it in the same receive, by putting a destination callback in the memo:

```json
{"dest_callback": {"address": "C…"}}
```

The address must be the transfer's receiver. The app credits the tokens to that contract, then calls:

```rust
fn on_ibc_transfer_received(env: Env, transfer_app: Address, received: TransferReceived);
```

`TransferReceived` carries the source and destination clients, the sequence, the sender on the source chain, the token's contract address on Stellar, the amount and the full memo, so the contract can read its own instructions from it. A contract should check that `transfer_app` is the transfer app it trusts and call `transfer_app.require_auth()`, which succeeds only when the transfer app is the caller.

If the call fails, the credit is rolled back and the packet is acknowledged with an error, so the sender is refunded on the source chain. A callback naming a contract other than the receiver, or a `dest_callback` the app cannot read, is refused the same way. Only the first 1,024 bytes of a memo are inspected; a longer memo is treated as a plain transfer.

## Ack / timeout handling on the source side

```
+--------------+      MsgAckPacket / MsgTimeoutPacket       +-------------+
|  relayer     |-------------------------------------------->|  IbcRouter |
+--------------+                                             +------+------+
                                                                    |
                                                            verify_membership /
                                                            verify_non_membership
                                                                    |
                                                                    v
                                                       on_acknowledgement_packet  / on_timeout_packet
                                                                    |
                                                                    v
                                                         +----------+------------+
                                                         |  IbcTransferApp       |
                                                         |   (this crate)        |
                                                         +----------+------------+
                                                                    |
                                                              if error ack:
                                                                refund sender
                                                              else (success ack):
                                                                no-op
                                                              if timeout:
                                                                refund sender
```
