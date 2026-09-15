# `stellar-mock-light-client` — accept-everything LC for development

Test-only light client. Mirrors the shared LC contract interface that the
router dials but skips all cryptographic checks — `check_for_misbehaviour`
returns `false`, `verify_membership` / `verify_non_membership` return `true`,
`get_timestamp_at_height` returns `0`.

## What it does

Exposes the 12 functions any LC must implement so the router can dial it via
`env.invoke_contract`. Storage is per-`client_id`:

- `DataKey::ClientState(client_id)` — opaque bytes
- `DataKey::ConsensusState(client_id, height)` — opaque bytes
- `DataKey::LatestHeight(client_id)` — `u64`
- `DataKey::Frozen(client_id)` — `bool`

`update_state` bumps the stored `latest_height` by 1 each call. Everything
else is a no-op or `true`.

## Who calls it

Only the router, via `env.invoke_contract` during:

- `create_client` → `initialise(client_id, client_state, consensus_state, height)`
- `update_client` → `check_for_misbehaviour` then `update_state` (or
  `update_state_on_misbehaviour` on conflict — never reached because
  misbehaviour is hard-coded to `false`)
- `recv_packet` → `verify_membership(client_id, height, proof, path, value)`
- `timeout_packet` → `get_timestamp_at_height(client_id, height)` +
  `verify_non_membership(...)`

Apps never call it directly.

## When to use it

- Unit tests for the router's packet lifecycle (every router test that
  exercises `recv_packet` / `acknowledge_packet` / `timeout_packet`
  registers this LC under `client_type = "mock"`).
- Unit tests for the transfer-app (same).
- Local manual smoke flows that don't have a real counterparty to verify
  against.

**Never deploy this to mainnet.** It accepts any proof.

## Entrypoints (13)

`initialise`, `latest_height`, `client_state`, `consensus_state`,
`verify_client_message`, `check_for_misbehaviour`, `update_state`,
`update_state_on_misbehaviour`, `frozen`, `verify_membership`,
`verify_non_membership`, `get_timestamp_at_height`, `client_type`.

The first 12 match the LC interface; `client_type()` returns the literal
symbol `"mock"` for caller introspection.

## Architecture flow

```
+-----------------+                              +--------------------+
|  IbcRouter      |  env.invoke_contract(...)    |  MockLightClient   |
|  (router crate) |----------------------------->|   (this crate)     |
|                 |     verify_membership        |                    |
|                 |<-----  always true  ---------|                    |
|                 |                              |                    |
|                 |     update_state             |                    |
|                 |<--- latest_height += 1 ------|                    |
+-----------------+                              +--------------------+
```

No external callers. No event emission. State lives entirely inside the LC's
own persistent storage.
