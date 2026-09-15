# `stellar-attestation-light-client` — m-of-n Ed25519 attestor LC

Production-grade light client that trusts a fixed off-chain attestor set
(m-of-n Ed25519 signatures over the attested data). Soroban port of Eureka's
`AttestationLightClient.sol`. Switches the signature scheme from secp256k1
(EVM-native) to Ed25519 (Stellar-native via `env.crypto().ed25519_verify`).

Used to verify state from any counterparty whose validators agree to attest —
useful when the counterparty doesn't run a Tendermint-style consensus or when
we want to bridge before the full Tendermint LC is ready.

## What it does

- Stores a fixed set of attestor public keys + a quorum threshold
  (`min_required_sigs`).
- On `update_state`: decodes an `AttestationProof`, verifies each Ed25519
  signature against the attestor index it claims, enforces uniqueness +
  quorum, decodes the `StateAttestation { height, timestamp }`, stores it as
  the ConsensusState at `height`.
- On `verify_membership` / `verify_non_membership`: takes an
  `AttestationProof` whose `attestation_data` is a `PacketAttestation
  { height, packets: [{path, value}, ...] }`, re-verifies the signatures, and
  asserts the requested `(path, value)` pair is (or isn't) in the attested
  list at the requested height.
- On `check_for_misbehaviour`: returns `true` iff a ConsensusState already
  exists at the message's height with a different timestamp.
- On `update_state_on_misbehaviour`: sets the `frozen` flag; subsequent
  state updates + proof verifications reject.

## Who calls it

Only the router (`stellar-ibc-router`), via `env.invoke_contract`. The
relayer-side flow is:

1. Off-chain attestors observe the counterparty chain and sign
   `(height, timestamp)` + per-packet `(path, value)` blobs.
2. Aggregator wraps signatures into `AttestationProof` and forwards to the
   relayer.
3. Relayer submits `MsgUpdateClient` / `MsgRecvPacket` etc. through the
   gateway's `StellarGatewayMsg`.
4. The router dispatches into this LC's `update_state` / `verify_membership`
   / `verify_non_membership`.

## When to use it

- Counterparty doesn't run Tendermint (or whatever the Tendermint LC will
  support).
- Fast path for Cosmos→Stellar packet flow before the full
  `stellar-tendermint-light-client` ships its crypto verifier.
- Testnet / private chains where m-of-n attestor governance is acceptable.

**Caveat:** attestor key rotation is not in scope. The set is pinned at
`initialise`. A rotation flow (committee elections, multi-attestation paths)
is future work.

## Wire shapes

```rust
struct AttestorClientState {
    attestor_keys: Vec<BytesN<32>>,
    min_required_sigs: u32,
    latest_height: u64,
    frozen: bool,
}

struct StateAttestation { height: u64, timestamp: u64 }

struct PacketCompact { path: Bytes, value: Bytes }
struct PacketAttestation { height: u64, packets: Vec<PacketCompact> }

struct AttestationProof {
    attestation_data: Bytes,          // XDR-encoded StateAttestation OR PacketAttestation
    signatures: Vec<BytesN<64>>,      // Ed25519 sigs over sha256(attestation_data) — implicit via host fn
    signer_indices: Vec<u32>,         // index into client_state.attestor_keys
}
```

`signatures[i]` must verify against `attestor_keys[signer_indices[i]]`.
Duplicate or out-of-range indices reject.

## Entrypoints (12)

`initialise`, `latest_height`, `client_state`, `consensus_state`,
`verify_client_message` (stub), `check_for_misbehaviour`, `update_state`,
`update_state_on_misbehaviour`, `frozen`, `verify_membership`,
`verify_non_membership`, `get_timestamp_at_height`.

## Architecture flow

```
+-------------------+
| off-chain         |
| attestor quorum   |
| (m signers from n)|
+--------+----------+
         |  sign(sha256(attestation_data))
         v
+--------+----------+        AttestationProof bytes
|  aggregator       |--------------------------------+
+-------------------+                                |
                                                    |
                                                    v
                                          +---------+----------+
                                          |  relayer (hermes)  |
                                          +---------+----------+
                                                    |  StellarGatewayMsg/{UpdateClient,RecvPacket,...}
                                                    v
                                          +---------+----------+
                                          |  stellar-hermes-   |
                                          |     gateway        |
                                          +---------+----------+
                                                    |  Soroban invoke
                                                    v
                                          +---------+----------+
                                          |  IbcRouter         |
                                          +---------+----------+
                                                    |  env.invoke_contract
                                                    v
                                          +---------+----------+
                                          | AttestationLight-  |
                                          |   Client (this)    |
                                          +--------------------+
                                              |       |
                                              v       v
                                       update_state   verify_membership
                                       (ConsensusState (asserts path/value
                                        per height)    is in attested list)
```
