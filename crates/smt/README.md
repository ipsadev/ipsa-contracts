# `ipsa-smt`: the router's sparse Merkle tree

The 64-level sparse Merkle tree the IBC router keeps, and the storage interface the router implements. It is a library with no contract of its own: the algorithm lives here, the persistence lives in `ibc-router/src/smt`.

```rust
pub const TREE_DEPTH: u32 = 64;

pub fn key_index(env: &Env, key: &Bytes) -> u64;
pub fn leaf_hash(env: &Env, key: &Bytes, value: &Bytes) -> BytesN<32>;
pub fn inner_hash(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32>;
pub fn empty(env: &Env) -> BytesN<32>;
pub fn is_empty(hash: &BytesN<32>) -> bool;

impl<'a, S: TreeStorage> Tree<'a, S> {
    pub fn new(env: &'a Env, storage: &'a S) -> Self;
    pub fn set(&self, index: u64, hash: &BytesN<32>);
    pub fn remove(&self, index: u64);
    pub fn root(&self) -> BytesN<32>;
}

pub fn compute_root(env: &Env, leaves: &Map<u64, BytesN<32>>) -> BytesN<32>;
```

A caller supplies a storage, binds the tree to it once, and works through that handle:

```rust
let storage = RouterStorage::new(env);
let tree = Tree::new(env, &storage);

tree.set(key_index(env, &key), &leaf_hash(env, &key, &value));
let root = tree.root();
```

`Tree` borrows the environment and the storage, and holds no state of its own.

## Hashing

A key's position is the first eight bytes of `sha256(key)`, read big endian, so the tree is 64 levels deep. Hashing is SHA-256 with a one-byte domain tag:

- a leaf is `sha256(0x00 || sha256(key) || sha256(value))`;
- an inner node is `sha256(0x01 || left || right)`.

The empty hash is 32 zero bytes. `inner_hash` returns it unchanged when both children are empty, rather than hashing zeros, so an empty subtree costs nothing and an empty tree's root is the empty hash. A node with one empty child is still folded, so a lone child never passes through unchanged.

## The compressed tree

Storing all 64 levels would cost a write per level per update. Instead the tree keeps only the nodes that distinguish the leaves it holds. A `Child` is either a `Leaf(index)` or a `Node(level, prefix)`, and a `Branch` holds its two children and their folded hash. `Top` names the highest child, which may be a bare leaf when the tree holds one key.

`root` lifts the top child to depth 64 by folding it against the empty hash on each level, choosing the side from the prefix bit. Inserting a key that lands below an existing child splits it at the first differing bit; removing a leaf collapses its branch back onto the sibling. The result is identical to folding every level, which `compute_root` does from a plain map of leaves and the tests use as the reference. No contract calls it and it is dropped from the contract binary.

Each `Branch` caches its folded hash, so the work stays proportional to the stored shape rather than the depth. `root` reads the top child and the one branch it names, then folds at most 64 hashes without touching storage again. A `set` or `remove` rewrites only the branches on its descent path, reading each one and its two children to refold it. That path is bounded by the number of distinct leaves and never exceeds 64.

## The storage interface

`TreeStorage` is the whole boundary. The algorithm reads and writes children, branches and leaves through it and never names a storage key, a TTL, or an error of its own:

```rust
pub trait TreeStorage {
    fn top(&self) -> Option<Child>;
    fn set_top(&self, top: Option<Child>);
    fn branch(&self, level: u32, prefix: u64) -> Option<Branch>;
    fn set_branch(&self, level: u32, prefix: u64, branch: &Branch);
    fn remove_branch(&self, level: u32, prefix: u64);
    fn leaf(&self, index: u64) -> Option<BytesN<32>>;
    fn has_leaf(&self, index: u64) -> bool;
    fn set_leaf(&self, index: u64, hash: &BytesN<32>);
    fn remove_leaf(&self, index: u64);
    fn corrupt(&self) -> !;
}
```

`corrupt` is how the implementation reports a tree that disagrees with itself, such as a branch the path reached but storage does not hold. The router raises its own `ProvableStoreCorrupt` there; the tests panic.

Writes land as they happen, not at the end: by the time `set` returns, the leaf, the branches on its path and the top child are all in storage, and `root` only reads what is already there.

What the router keeps, and this crate does not: the `SmtKey` storage keys, the persistent TTL, the `Owner` entry that stops two keys sharing a truncated index, the committed root, and the `IbcRoot` event. The router's own `smt` module exposes only those router-owned pieces, so code that wants the algorithm imports `ipsa_smt` directly rather than going through the router.

## Tests

`tests/hashing.rs` (6 tests) pins the tagged-pair construction, the empty-child collapse, the domain separation between a leaf and an inner node, child order sensitivity, and the big-endian key index.

`tests/roots.rs` (11 tests) runs the compressed tree against `compute_root` across insert, overwrite, remove, reinsert, varying insertion order and sparse distant indices (`0`, `1`, `1 << 63` and `u64::MAX`). It also holds the shared vectors the router has always produced, asserted twice over, once through `compute_root` and once through the compressed tree: `389440af…` for one packet commitment and `5ec97eda…` for a commitment, receipt and ack.

The router's own integration tests live in `ibc-router/tests`: `smt.rs` pins the `Child`, `Branch` and `SmtKey` encodings and the lag between a written leaf and a committed root, `slots.rs` covers the `Owner` entry and the keys that collide on a truncated index, and `incremental_smt.rs` replays random and edge-index operation sequences against a mirror map, including a budget check that one write into a large tree still fits a single transaction.
