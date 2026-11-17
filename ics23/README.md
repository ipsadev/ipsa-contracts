# `stellar-ics23`: ICS-23 proofs for Soroban

Verifies ICS-23 membership and non-membership proofs from a Cosmos SDK chain against a 32-byte app hash, in a Soroban contract. The Tendermint light client depends on it, so header verification and proof verification live in separate crates. It is a library, with no contract of its own.

```rust
pub const PROOF_LEVELS: u32 = 2;

pub fn verify_membership(env: &Env, root: &[u8; 32], proof: &Bytes, keys: &Vec<Bytes>, value: &Bytes) -> bool;

pub fn verify_non_membership(env: &Env, root: &[u8; 32], proof: &Bytes, keys: &Vec<Bytes>) -> bool;
```

Both return `false` for any proof they do not accept, including bytes that do not decode.

## Path layout

`proof` is an ibc-go `MerkleProof`: a protobuf message whose field 1 repeats one `CommitmentProof` per level, innermost first. A Cosmos SDK chain has two levels:

- level 0, the IAVL store that holds the IBC state;
- level 1, the multistore, whose root is the app hash.

`keys` is an ibc-go `MerklePath`, outermost first, so it is in the opposite order to the proofs. It must hold exactly one key per proof level:

- `keys[0]` is the store name, `ibc`, which the multistore proof must prove;
- `keys[1]` is the key inside the IBC store, which the IAVL proof must prove (or prove absent).

Every level is checked against its own key. Joining the keys and comparing the result to the joined proof keys is not enough: `ib` + `c…` and `ibc` + `…` give the same bytes, but only the second names the `ibc` store.

The router builds `keys` from the counterparty's registered commitment prefix the way ibc-go's `BuildMerklePath` does: every prefix segment but the last is its own key, and the packet key is appended to the last one. A Cosmos counterparty is registered with `["ibc", ""]`, which gives `["ibc", key]`.

## Specs it enforces

Each level is checked against its proof spec, as ibc-go's `iavl_spec` (level 0) and `tendermint_spec` (level 1) define them:

- **Leaf.** The hash is SHA-256, the key is not prehashed, the value is prehashed with SHA-256, lengths are varint-prefixed, and the leaf prefix starts with `0x00`. At level 0 the leaf prefix must also be a well-formed IAVL height, size and version with nothing after them.
- **Inner nodes.** The hash is SHA-256, and the prefix must not start with the leaf prefix `0x00`. The prefix length must be within the spec's bounds (4 to 12 bytes plus one child for IAVL, exactly 1 byte plus one child for Tendermint), and the suffix must be a whole number of children (33 bytes for IAVL, 32 for Tendermint). At level 0 the IAVL height in each inner prefix must be at least the node's depth from the leaf, and the size and version must not be negative.
- **Keys and values.** An existence proof must have a non-empty key and value. Level 0 must prove the value asked about, and level 1 must prove level 0's root as its value.
- **Non-membership.** Level 0 must be a `NonExistenceProof` whose neighbours are adjacent in the tree:
  - a left and right pair that meet at one inner node, with the left neighbour the rightmost leaf below that node's left child and the right neighbour the leftmost leaf below its right child;
  - or a lone left neighbour that is the rightmost leaf in the tree;
  - or a lone right neighbour that is the leftmost leaf in the tree.

  The left key must sort before the absent key and the right key after it. Level 1 must be an existence proof of level 0's root.
- **Shape.** There must be exactly two proof levels and exactly two keys. A third level, or a proof of the wrong kind at a level, is refused.

## Tests

`tests/verify.rs` builds IAVL trees and multistores with the `ics23` crate's types and checks each case against the `ics23` crate as a reference. `tests/hub_testnet.rs` checks a real acknowledgement commitment and a real absent receipt taken from the Cosmos Hub testnet (`08-wasm-464` at height 19878906, app hash from block 19878907). It shows that `["ibc", ""]` proves them, and that the legacy single segment `["ibc"]` and the split `["ib", "c"]` do not. The shared builders in `tests/common` are also used by the workspace's `tests/` crate.
