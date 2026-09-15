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
    XDR-encodes a `FungibleTokenPacketData`, packages it into a `Payload`
    (port=`transfer`, version=`ics20-2`, encoding=`xdr`), and calls
    `router.send_packet` via `env.invoke_contract`. Returns the minted
    sequence.
- **Inbound (counterparty → user):**
  - `on_recv_packet(callback)` is dispatched by the router; the app decodes
    the packet data, credits the receiver's local balance, and returns the
    success ack `[0x01]`.
- **Ack handling:**
  - `on_acknowledgement_packet(callback)` is dispatched by the router after
    the counterparty's ack arrives. If the ack matches the success sentinel
    `[0x01]`, no-op. Otherwise refund the sender from escrow.
- **Timeout handling:**
  - `on_timeout_packet(callback)` is dispatched by the router when the
    packet times out without delivery; the app refunds the sender from
    escrow.

## Who calls it

| Caller | Calls |
|---|---|
| Off-chain admin (deploy) | `__constructor(router, admin)`, `set_rate_limit(denom, daily_cap)` |
| Users (wallets, scripts) | `initiate_transfer(...)`, `balance_of(...)`, `daily_cap(...)`, `daily_usage(...)` |
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

The `Payload.value` is the XDR encoding of `FungibleTokenPacketData`.

## Entrypoints (10)

- `__constructor(router, admin)`
- `set_rate_limit(denom, daily_cap)` — admin-only
- `daily_cap(denom) -> Option<i128>` / `daily_usage(denom) -> i128`
- `mint(to, denom, amount)` / `balance_of(who, denom) -> i128`
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
       returns ack = [0x01]
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
