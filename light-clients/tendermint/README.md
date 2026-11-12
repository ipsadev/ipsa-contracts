# `stellar-tendermint-light-client` — Tendermint light client for Stellar

Verifies Cosmos chain headers and IAVL membership proofs on Stellar. It is the Soroban counterpart of ibc-go's `07-tendermint` client: it checks Ed25519 commit signatures and ICS-23 proofs directly in the contract, without a zk verifier.

## Updating the client

`update_state` accepts a `TendermintHeader` for a height above the trusted one when all of these hold:

- the header's chain id is the client's
- the header hashes to the block id the commit signed
- the header's validator set hashes to its `validators_hash`, and validators holding more than 2/3 of its voting power signed the commit
- the trusted consensus state is still inside the trusting period

The header must also connect to the trusted state, in one of two ways, as ibc-go's `VerifyAdjacent` and `VerifyNonAdjacent` do:

- **Same validators.** The header's `validators_hash` is the trusted state's `next_validators_hash`. This is the only way an adjacent header (trusted height + 1) is accepted.
- **Changed validators.** The header carries `trusted_validator_set_bytes`, the validator set at the trusted height + 1, which must hash to the trusted state's `next_validators_hash`. Validators from that set holding more than the client's `trust_level` (1/3 by default) of its voting power must have signed the commit. A trusted validator counts only when it signed with the same key in the new set.

The second way is what keeps the client moving when the chain's validator set changes: without it, every header after the change carries a validator set the client does not trust, and the client can never be updated again. Its safety rests on the usual light client assumption that fewer than 1/3 of the trusted voting power is dishonest, so more than 1/3 signing means an honest trusted validator vouches for the header.

A header without `trusted_validator_set_bytes`, the shape gateways sent before it existed, still decodes, and can only update the client while the validator set is unchanged.

On the Cosmos Hub testnet, with 113 validators, an update across a validator set change costs about 152M CPU instructions in the wasm, and an update with an unchanged set about 118M. The network allows 400M per transaction.

## Rejection reasons

A header that fails verification panics with a distinct `Error` so a caller, a test, or an on-chain negative-path check can tell which invariant it broke rather than reading one opaque failure. The checks run in this order, and the first one to fail decides the code:

- `ChainIdMismatch` (8) — the header's chain id is not the client's.
- `HeaderHeightNotAfterTrusted` (9) — the header is at or below the trusted height.
- `ValidatorSetHashMismatch` (16) — the supplied validator set does not hash to the header's `validators_hash`.
- `CommitHeightMismatch` (17) — the commit is for a different height than the header.
- `BlockHashMismatch` (18) — the header does not hash to the block id the commit signed.
- `InsufficientVotingPower` (20) — the signatures present carry 2/3 or less of the set's voting power.
- `TrustLevelNotMet` (21) — a changed-validator header is adjacent, or fewer than the client's `trust_level` of the trusted voting power signed it.
- `RevisionMismatch` (14) — the header's declared `target_height.revision_number` is not the trusted revision.
- `TargetHeightMismatch` (22) — the header's declared `target_height.revision_height` is not the height that was actually verified.
- `InvalidHeader` (5) — the bytes are malformed, or a commit signature's position does not match its validator.
- `BadCommitSignature` (19) — a commit signature is not 64 bytes. A well-formed but cryptographically wrong Ed25519 signature traps in the host `ed25519_verify` instead of returning this code; giving that case a typed error needs a non-trapping host verify.

## Proofs

`verify_membership` and `verify_non_membership` walk an ICS-23 IAVL proof against the root stored for the height, and refuse when the height has no consensus state or the client is frozen.

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
    trusted_validator_set_bytes: Bytes,
}

struct Misbehaviour { header_a: TendermintHeader, header_b: TendermintHeader }
```

`signed_header_bytes` and the two validator set fields are the protobuf `SignedHeader` and `ValidatorSet` messages. `trusted_validator_set_bytes` may be empty when the validator set has not changed.

## Entrypoints

`initialise`, `latest_height`, `client_state`, `consensus_state`, `verify_client_message`, `check_for_misbehaviour`, `update_state`, `update_state_on_misbehaviour`, `frozen`, `verify_membership`, `verify_non_membership`, `get_timestamp_at_height`.

`check_for_misbehaviour` reports a verified header whose root differs from the one already stored for its height.

## Access

The contract is deployed with the router's address (`__constructor(router)`), which becomes its OpenZeppelin access-control admin. `initialise` and `update_state_on_misbehaviour` are `#[only_admin]`, so only the router reaches them: `create_client` and `migrate_client` for the first, and `update_client` after `check_for_misbehaviour` for the second. A client id is initialised once; a second `initialise` for it panics with `ClientAlreadyInitialised`.

The contract has no `upgrade`: a deployed client moves to new code through the router's `migrate_client`, which initialises a new light client for the same client id with its current client and consensus state.

## Flow

```
[Cosmos chain: signed headers and IAVL proofs]
        |
        v
relayer --(UpdateClient, RecvPacket, ...)--> gateway --(Soroban invoke)--> IbcRouter
                                                                              |
                                                                     env.invoke_contract
                                                                              v
                                                              TendermintLightClient (this crate)
                                                              update_state: commit signatures
                                                              verify_membership: ICS-23 walk
```

## When to use it

For a Cosmos counterparty that runs Tendermint and IAVL (simd, the Cosmos Hub, Osmosis and so on), registered under the client type `07-tendermint`.
