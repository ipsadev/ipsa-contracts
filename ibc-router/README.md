# `stellar-ibc-router` — ICS-26 packet router for Stellar

The core IBC v2 router contract. Equivalent of Eureka's `ICS26Router.sol`
combined with the `ICS02ClientUpgradeable` (client registry) and
`IBCStoreUpgradeable` (commitment store) mixins. Lives at
`contracts/soroban/ibc-router/`.

## What it does

- Owns the client registry: which `client_type` (e.g. `mock`, `attestation`,
  `07-tendermint`) maps to which light-client contract address, and which
  `client_id` (e.g. `mock-0`, `07-tendermint-0`) was minted from which type.
- Owns the counterparty mapping per client (the counterparty's `client_id` +
  ICS-2 §RegisterCounterparty commitment prefix).
- Owns the port router: which app contract handles each port (e.g.
  `transfer` → `IbcTransferApp`).
- Owns the provable storage at the three ICS-24 v2 paths: packet commitments,
  receipts, and ack commitments — keyed by the literal v2 path bytes
  (`{client_id} || disc || be64(seq)`) so the gateway's SMT mirror can re-key
  them without any router-internal knowledge.
- Implements the five packet-lifecycle entrypoints: `send_packet`,
  `recv_packet`, `write_acknowledgement`, `acknowledge_packet`, `timeout_packet`.

## Who calls it

| Caller | Calls |
|---|---|
| Off-chain admin (deploy + bootstrap) | `__constructor`, `register_client_type`, `register_counterparty`, `register_port` |
| IBC application contracts (e.g. `IbcTransferApp`) | `send_packet` (to mint a sequence + commit) |
| Relayer (via the gateway's `StellarGatewayMsg`) | `update_client`, `recv_packet`, `acknowledge_packet`, `timeout_packet` |
| Light-client contracts | Called *by* the router into them — not the reverse |
| App contracts during recv/ack/timeout dispatch | Called *by* the router into them via `on_recv_packet` / `on_acknowledgement_packet` / `on_timeout_packet` |

## Entrypoints (23)

**Client lifecycle (ICS-02):**
- `register_client_type(client_type, lc_address)` — admin pins an LC contract
  address under a client_type string.
- `lc_address(client_type) -> Option<Address>`
- `create_client(client_type, client_state, consensus_state, height) -> client_id`
  — mints `{client_type}-{N}`, invokes the LC's `initialise`.
- `register_counterparty(client_id, counterparty_client_id, prefix: Vec<Bytes>)`
  — ICS-2 §RegisterCounterparty; rejects duplicate.
- `counterparty(client_id) -> Option<Counterparty>`
- `update_client(client_id, client_message) -> u64` — checks LC misbehaviour,
  dispatches to `update_state` or `update_state_on_misbehaviour`.
- `client_lc_address(client_id) -> Option<Address>`
- `frozen(client_id) -> bool`

**Port router (ICS-05):**
- `register_port(port_id, app_address)` — app proves it owns itself via
  `require_auth`.
- `port_app(port_id) -> Option<Address>`

**Provable storage primitives (ICS-24):**
- `set_packet_commitment` / `packet_commitment` / `delete_packet_commitment`
- `set_packet_receipt` / `has_packet_receipt`
- `set_ack_commitment` / `acknowledgement`

**Packet lifecycle (ICS-04 / ICS-26):**
- `send_packet(source_client_id, timeout_timestamp, payloads) -> sequence`
- `recv_packet(packet, proof, proof_height)`
- `write_acknowledgement(dest_client_id, sequence, acks)`
- `acknowledge_packet(packet, acks, proof, proof_height)`
- `timeout_packet(packet, proof, proof_height)`

## IBC v2 flow — outbound (Stellar → counterparty)

```
+---------------+   initiate_transfer    +-----------------+
|  user wallet  |----------------------->|  IbcTransferApp |
+---------------+                        +--------+--------+
                                                  |
                                       send_packet | (port_app auths)
                                                  v
                                         +--------+--------+
                                         |  IbcRouter      |
                                         |   (this crate)  |
                                         +---+---+---+-----+
                            commit_v2_packet  |   |   |
                                              |   |   +--> SendPacket event
                                              |   +------> set_packet_commitment
                                              +----------> NextSeqSend++ (returns sequence)
                                              
                                          [relayer reads SendPacket event]
                                                  |
                                                  v
                                  [submits MsgRecvPacket to counterparty]
```

## IBC v2 flow — inbound (counterparty → Stellar)

```
[relayer]
   |  MsgRecvPacket (via gateway StellarGatewayMsg/RecvPacket)
   v
+----------------+                                       +-----------------+
|  IbcRouter     |---verify_membership(proof, path) ---->|  LightClient    |
|   (this crate) |                                       |  contract       |
+--------+-------+                                       +-----------------+
         |
         | set_packet_receipt
         |
         v
+----------------+                                       +-----------------+
|  IbcRouter     |---on_recv_packet(callback) ---------->|  IBC app        |
|                |<--ack bytes ------------------------- |  (transfer/...)|
+--------+-------+                                       +-----------------+
         |
         | commit_v2_acknowledgement + set_ack_commitment
         | emit RecvPacket + WriteAck events
         v
   [relayer picks up ack proof, calls AckPacket on source chain]
```

## Storage shape

Persistent storage uses two key families:

- **Enum keys** (`DataKey::ClientType(id)`, `DataKey::Counterparty(id)`,
  `DataKey::Port(id)`, `DataKey::NextSeqSend(id)`, etc.) — internal state the
  gateway's SMT mirror skips.
- **Bytes keys** (the v2 ICS-24 path bytes themselves) — for commitment,
  receipt, and ack entries. The gateway's `state_tracker` matches on
  `ScVal::Bytes` keys and mirrors them into the SMT verbatim.
