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

### Lifecycle rules

Three rules bound when an update is in time and whom it may trust. Each has a boundary that the tests pin on both sides.

- Trusting period. An update is accepted only while the ledger time is strictly before the trusted consensus state's timestamp plus the client's `trusting_period_secs`; at or past that point it is refused with `TrustingPeriodElapsed`. Freshness is measured from the trusted state the header builds on, not from the header. An update one second before the boundary is accepted; at the boundary it is refused.
- Clock drift and time order. The header's time must be strictly after the trusted state's time, otherwise `HeaderTimeNotAfterTrusted`, and strictly before the ledger time plus `max_clock_drift_secs`, otherwise `HeaderTimeInFuture`. A header ahead of the ledger is accepted up to the drift and refused at or beyond it.
- Adjacency and trust level. An adjacent header, at the trusted height plus one, is accepted only on the unchanged-validators path, where its validator set hashes to the trusted state's next validators hash; a changed set at an adjacent height is refused with `TrustLevelNotMet`. A non-adjacent header is accepted when trusted validators holding strictly more than `trust_level` of the trusted voting power signed it, and refused with `TrustLevelNotMet` otherwise. Exactly the trust level is not enough: a third of the trusted power is refused, just over a third is accepted.

### The router's two-step update, and why the order matters

The router drives an update in a single invocation: it calls `check_for_misbehaviour` first, then `update_state`. `check_for_misbehaviour` is where the header is fully verified (the checks above); when it passes it records the sha256 of the update message together with the current ledger sequence. `update_state` does not verify the header a second time. It applies a header only when that header is the one `check_for_misbehaviour` verified for this client in the same ledger: the recorded hash must match the message and the recorded ledger must be the current one. It then consumes the record. Anything else is refused with `HeaderNotVerified`.

So a header is verified exactly once per update, and `update_state` cannot be made to apply a header that was never verified, even though it is only reachable through the admin (the router). Soroban has no storage that clears at the end of a call, so the record is bound to the ledger (which closes about every five seconds) and consumed on use: a record left behind by a `check_for_misbehaviour` that no `update_state` followed stops authorising anything as soon as the next ledger closes.

### Misbehaviour

`check_for_misbehaviour` is where the router decides, inside `update_client`, whether a submitted message freezes the client instead of advancing it. Submission is permissionless: `update_client` is ungated, so anyone can relay evidence, and only the freeze itself (`update_state_on_misbehaviour`) is admin-only, reached by the router once detection returns true. Every header named below is fully verified first, by its signatures, trust level, chain id and the trusting period, so an unverifiable or stale header reverts rather than freezing anything, and a frozen client reports no further misbehaviour.

It detects three forms, mirroring ibc-go's `CheckForMisbehaviour`:

- a single verified header that contradicts the consensus state already stored at its height, in its app hash, its timestamp or its next validators hash. Two different blocks were finalised at one height, which a correct chain never does. This is the same-height fork arriving one header at a time, so the whole stored consensus state is compared, not only its root, and a header that merely repeats the stored state is not misbehaviour.
- a `Misbehaviour` message carrying two verified headers. Two headers at one height with different block hashes are a fork. Two headers at different heights whose timestamps run backwards, the higher header carrying a time strictly before the lower one, are a violation of monotonic (BFT) time.
- a single verified header whose timestamp breaks monotonicity with its stored neighbours: strictly before the nearest stored height below it, or strictly after the nearest stored height above it. Tendermint time is increasing with height, so either case is evidence the chain signed inconsistent times.

The two headers of a `Misbehaviour` message are evidence, not updates, so they skip the two time rules an ordinary update applies against its trusted state (header time after the trusted time, header time within clock drift of now). Those rules would otherwise make a genuine time violation revert instead of freezing. The headers are still checked for valid signatures, trust level, chain id and a trusted state within the trusting period.

Timestamps are compared at whole-second resolution, because the stored consensus state keeps only the seconds of a Tendermint time. Every comparison above is therefore strict: two headers that share a second are never a time violation. Without this, any chain producing more than one block per second would let anyone freeze a client by wrapping two honest consecutive headers, since submission is permissionless. The cost is that a sub-second time violation is not caught through time alone; a fork at one height is still caught by its block hash regardless of time.

The lone-header form needs the neighbours of an arbitrary height, which Soroban storage cannot range-scan, so the client keeps a sorted index of the heights it holds consensus states for, extended on `initialise`, `update_state` and `recover`. The index is bounded at the most recent 1000 heights; older entries drop off. A fast-updating client can hold more than that within its trusting period, so a dropped height may still be a usable trust anchor: dropping it from the index does not remove or weaken its consensus state, which stays stored, and the same-height fork check at that height still works. All that is lost is a neighbour for the time-monotonicity check of a header landing near the oldest heights. The index is built from `initialise` onward, and since a client only reaches new code through the router's `migrate_client`, which re-initialises it with its current state, the index always matches the states the client holds. A `Misbehaviour` message is told apart from a lone header by its `header_a` field; a two-header message that turns out not to be misbehaviour is rejected with `NotMisbehaviour` rather than falling through to an update.

### Recovery

`recover(subject_client_id, substitute_client_id)` heals a client that can no longer be updated, one frozen by misbehaviour or expired past its trusting period, from a healthy substitute of the same chain. It is how ibc-go's governance client recovery works, narrowed to a same-chain substitute. The substitute must be another client on this same contract: `recover` reads both from its own storage. It enforces the same conditions ibc-go's `RecoverClient` does, so recovery cannot copy stale state or heal a client that was never stuck:

- the subject must be beyond an ordinary update, frozen or expired past its trusting period, otherwise `SubjectStillActive`. A client that can still be updated has no need of recovery.
- the substitute must be strictly higher than the subject, otherwise `SubstituteNotNewer`. A substitute no newer than the subject heals nothing.
- the substitute must itself be live: not frozen, otherwise `ClientFrozen`, and not expired past its trusting period, otherwise `SubstituteExpired`. A frozen or expired substitute would copy stale state.
- both must be the same chain, otherwise `ChainIdMismatch`.
- the substitute's safety parameters must match the subject's: trust level, unbonding period, clock drift and proof specs, otherwise `SubstituteParamsMismatch`. Only the latest height, frozen height and trusting period may differ, as in ibc-go's `IsMatchingClientState`. Without this a weaker substitute, which anyone can create since `create_client` is permissionless, could launder a state it verified under looser rules into a client that still reports the stricter ones.
- the substitute's trusting period must be below the subject's unbonding period, otherwise `TrustingPeriodNotBelowUnbonding`, since the subject takes the substitute's trusting period and `initialise` does not check this.

Expiry is measured the same way an update measures it: the client's latest consensus timestamp plus its trusting period, against the ledger time. When every condition holds, `recover` copies the substitute's consensus state at its latest height onto the subject, sets the subject's latest height and trusting period to the substitute's, zeroes the frozen height, and clears the frozen flag. The subject keeps its own trust level, unbonding period, clock drift and proof specs. The subject can be updated again from the substitute's height, with a fresh consensus state inside the trusting period. `recover` is `#[only_admin]`, so only the router reaches it, through `recover_client`.

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

`verify_membership` and `verify_non_membership` take the proof and the path as a list of keys, one per proof level, and check them against the root stored for the height with the [`ipsa-ics23`](../../ics23) crate. They refuse when the height has no consensus state or the client is frozen. The crate's README lists the proof specs it enforces and the path layout it expects. `proof_levels` returns 2, the number of keys the router must pass, so the router refuses a counterparty prefix of any other length when it is registered.

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

`initialise`, `latest_height`, `client_state`, `consensus_state`, `verify_client_message`, `check_for_misbehaviour`, `update_state`, `update_state_on_misbehaviour`, `recover`, `frozen`, `verify_membership`, `verify_non_membership`, `proof_levels`, `get_timestamp_at_height`.

`check_for_misbehaviour` reports a verified header whose root differs from the one already stored for its height.

## Access

The contract is deployed with the router's address (`__constructor(router)`), which becomes its OpenZeppelin access-control admin. `initialise`, `update_state_on_misbehaviour` and `recover` are `#[only_admin]`, so only the router reaches them: `create_client` and `migrate_client` for the first, `update_client` after `check_for_misbehaviour` for the second, and `recover_client` for the third. A client id is initialised once; a second `initialise` for it panics with `ClientAlreadyInitialised`.

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
                                                              verify_membership: ipsa-ics23
```

## When to use it

For a Cosmos counterparty that runs Tendermint and IAVL (simd, the Cosmos Hub, Osmosis and so on), registered under the client type `07-tendermint`.
