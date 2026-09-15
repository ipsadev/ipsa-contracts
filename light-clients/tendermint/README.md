# `stellar-tendermint-light-client` — Tendermint LC scaffold (⚠️ crypto deferred)

Light client that verifies Cosmos chain headers + IAVL membership proofs on
Stellar. Soroban analog of Eureka's `SP1ICS07Tendermint.sol`, but **without**
the SP1 zk verifier — Soroban's WASM budget is generous enough for direct
header verification once the no_std crypto stack is wired in.

**Status today:** scaffold. The data model, storage layout, height
monotonicity check, frozen propagation, and conflicting-root misbehaviour
detection are all live. **Real cryptographic verification — Tendermint
signed-header check + ICS-23 IAVL proof walking — is stubbed at the
`TODO(crypto)` boundary** and currently accepts any well-formed input. See
the deferred-work section below.

## What it does today (structural)

- Stores a Tendermint-shaped `ClientState { chain_id, trust_level,
  trusting_period_secs, unbonding_period_secs, max_clock_drift_secs,
  latest_height, is_frozen, frozen_height, proof_specs }`.
- Stores per-height `ConsensusState { timestamp_secs, next_validators_hash,
  root }`.
- `update_state`: rejects non-advancing heights + frozen clients, stores the
  new ConsensusState keyed by `target_height`, advances `latest_height`.
- `check_for_misbehaviour`: returns `true` iff a ConsensusState exists at the
  header's target height with a **different** root (same root is idempotent).
- `update_state_on_misbehaviour`: sets `is_frozen` + `frozen_height`.
- `verify_membership` / `verify_non_membership`: reject if height has no
  ConsensusState or client is frozen, then **accept any other input** (stub).
- `get_timestamp_at_height`: reads `ConsensusState.timestamp_secs`.

## What it doesn't do yet (cryptographic)

Three call sites carry the production verifier — they're currently no-ops:

1. `update_state` — header signature verification:
   - decode `signed_header_bytes`, confirm `chain_id` matches `cs.chain_id`
   - verify Ed25519 quorum (⅔+ voting power per `trust_level`)
   - verify `next_validators_hash` chains from the trusted ConsensusState
   - reject if `(env.ledger().timestamp() - trusted_timestamp) > trusting_period`
2. `verify_membership` — ICS-23 IAVL proof walk against
   `ConsensusState.root` using the spec embedded in `client_state.proof_specs`.
3. `verify_non_membership` — same, for the absence path.

Blocked on: `tendermint = "0.40"` and `ics23 = "0.12"` (workspace deps) are
std-heavy and won't compile to `wasm32v1-none`. Needs either a no_std fork or
a hand-rolled minimal verifier.

## Who calls it

Same as any LC — only the router, via `env.invoke_contract`. When the
crypto verifier lands, the call sites don't change; the structure is already
in place.

## When to use it

- Cosmos counterparty (simd, Cosmos Hub, Osmosis, etc.) where
  the chain runs Tendermint and IAVL.
- Registered under `client_type = "07-tendermint"` for the standard naming
  convention.

Until the crypto lands, use this LC for: data-model integration tests,
router smoke flows that depend on the `07-tendermint` client_type, structural
misbehaviour tests. **Do not** use for any flow that needs to actually
verify a Cosmos packet — fall back to the attestation LC or mock LC.

## Wire shapes

```rust
struct TrustThreshold { numerator: u32, denominator: u32 }

struct Height { revision_number: u64, revision_height: u64 }

struct TendermintClientState {
    chain_id: String,
    trust_level: TrustThreshold,
    trusting_period_secs: u64,
    unbonding_period_secs: u64,
    max_clock_drift_secs: u64,
    latest_height: Height,
    is_frozen: bool,
    frozen_height: Height,
    proof_specs: Bytes,
}

struct TendermintConsensusState {
    timestamp_secs: u64,
    next_validators_hash: BytesN<32>,
    root: BytesN<32>,
}

struct TendermintHeader {
    trusted_height: Height,
    target_height: Height,
    timestamp_secs: u64,
    next_validators_hash: BytesN<32>,
    app_hash: BytesN<32>,
    signed_header_bytes: Bytes,
    validator_set_bytes: Bytes,
}

struct Misbehaviour { header_a: TendermintHeader, header_b: TendermintHeader }
```

## Entrypoints (12)

`initialise`, `latest_height`, `client_state`, `consensus_state`,
`verify_client_message` (stub), `check_for_misbehaviour`, `update_state`,
`update_state_on_misbehaviour`, `frozen`, `verify_membership`,
`verify_non_membership`, `get_timestamp_at_height`.

## Architecture flow

```
                          [Cosmos chain produces SignedHeader + IAVL proofs]
                                                |
                                                v
                                  +-------------+--------------+
                                  |  Cosmos relayer (hermes)   |
                                  +-------------+--------------+
                                                |  StellarGatewayMsg/{UpdateClient,RecvPacket,...}
                                                v
                                  +-------------+--------------+
                                  |  stellar-hermes-gateway    |
                                  +-------------+--------------+
                                                |  Soroban invoke
                                                v
                                  +-------------+--------------+
                                  |  IbcRouter                 |
                                  +-------------+--------------+
                                                |  env.invoke_contract
                                                v
                                  +-------------+--------------+
                                  | TendermintLightClient      |
                                  |   (this crate)             |
                                  +------+--------------+------+
                                         |              |
                                  update_state    verify_membership
                                         |              |
                                  +------v------+ +----v----------------+
                                  | TODO(crypto):| | TODO(crypto):       |
                                  | Ed25519 quorum| | ICS-23 IAVL walk    |
                                  | over          | | against root +      |
                                  | signed_header | | proof_specs         |
                                  +-------------+- +---------------------+
```
