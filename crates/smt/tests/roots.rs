use core::cell::RefCell;
use std::collections::BTreeMap;

use ipsa_smt::{compute_root, empty, key_index, leaf_hash, Branch, Child, Tree, TreeStorage};
use soroban_sdk::{Bytes, BytesN, Env, Map};

#[derive(Default)]
struct TestStorage {
    top: RefCell<Option<Child>>,
    branches: RefCell<BTreeMap<(u32, u64), Branch>>,
    leaves: RefCell<BTreeMap<u64, BytesN<32>>>,
}

impl TreeStorage for TestStorage {
    fn top(&self) -> Option<Child> {
        self.top.borrow().clone()
    }

    fn set_top(&self, top: Option<Child>) {
        *self.top.borrow_mut() = top;
    }

    fn branch(&self, level: u32, prefix: u64) -> Option<Branch> {
        self.branches.borrow().get(&(level, prefix)).cloned()
    }

    fn set_branch(&self, level: u32, prefix: u64, branch: &Branch) {
        self.branches
            .borrow_mut()
            .insert((level, prefix), branch.clone());
    }

    fn remove_branch(&self, level: u32, prefix: u64) {
        self.branches.borrow_mut().remove(&(level, prefix));
    }

    fn leaf(&self, index: u64) -> Option<BytesN<32>> {
        self.leaves.borrow().get(&index).cloned()
    }

    fn has_leaf(&self, index: u64) -> bool {
        self.leaves.borrow().contains_key(&index)
    }

    fn set_leaf(&self, index: u64, hash: &BytesN<32>) {
        self.leaves.borrow_mut().insert(index, hash.clone());
    }

    fn remove_leaf(&self, index: u64) {
        self.leaves.borrow_mut().remove(&index);
    }

    fn corrupt(&self) -> ! {
        panic!("the tree storage is corrupt")
    }
}

fn insert(env: &Env, tree: &Tree<TestStorage>, key: &[u8], value: &[u8]) {
    let key = Bytes::from_slice(env, key);
    let value = Bytes::from_slice(env, value);

    tree.set(key_index(env, &key), &leaf_hash(env, &key, &value));
}

fn drop_key(env: &Env, tree: &Tree<TestStorage>, key: &[u8]) {
    tree.remove(key_index(env, &Bytes::from_slice(env, key)));
}

fn leaf_set(env: &Env, entries: &[(&[u8], &[u8])]) -> Map<u64, BytesN<32>> {
    let mut leaves: Map<u64, BytesN<32>> = Map::new(env);

    for (key, value) in entries {
        let key = Bytes::from_slice(env, key);
        let value = Bytes::from_slice(env, value);
        leaves.set(key_index(env, &key), leaf_hash(env, &key, &value));
    }

    leaves
}

fn compressed_root(env: &Env, entries: &[(&[u8], &[u8])]) -> BytesN<32> {
    let storage = TestStorage::default();
    let tree = Tree::new(env, &storage);

    for (key, value) in entries {
        insert(env, &tree, key, value);
    }

    tree.root()
}

fn hex32(hash: &BytesN<32>) -> String {
    use core::fmt::Write;

    let mut out = String::new();

    for byte in hash.to_array() {
        let _ = write!(out, "{byte:02x}");
    }

    out
}

fn v2_path(env: &Env, client: &str, discriminator: u8, sequence: u64) -> Bytes {
    let mut key = client.as_bytes().to_vec();

    key.push(discriminator);
    key.extend_from_slice(&sequence.to_be_bytes());

    Bytes::from_slice(env, &key)
}

#[test]
fn an_empty_tree_has_a_zero_root() {
    let env = Env::default();
    let storage = TestStorage::default();

    assert_eq!(compute_root(&env, &Map::new(&env)), empty(&env));
    assert_eq!(Tree::new(&env, &storage).root(), empty(&env));
}

#[test]
fn shared_smt_vectors() {
    let env = Env::default();

    let commitment = v2_path(&env, "10-stellar-0", 0x01, 7);
    let commitment_value = Bytes::from_slice(&env, &[0xABu8; 32]);

    let mut leaves: Map<u64, BytesN<32>> = Map::new(&env);
    leaves.set(
        key_index(&env, &commitment),
        leaf_hash(&env, &commitment, &commitment_value),
    );

    assert_eq!(
        hex32(&compute_root(&env, &leaves)),
        "389440afec9968073f33047acc29c80207ba145947177b5bbe46edf836b6e666",
        "one packet commitment"
    );

    let receipt = v2_path(&env, "10-stellar-0", 0x02, 7);
    let receipt_value = Bytes::from_slice(&env, &[0x01u8]);
    leaves.set(
        key_index(&env, &receipt),
        leaf_hash(&env, &receipt, &receipt_value),
    );

    let ack = v2_path(&env, "10-stellar-0", 0x03, 7);
    let ack_value = Bytes::from_slice(&env, &[0xCDu8; 32]);
    leaves.set(key_index(&env, &ack), leaf_hash(&env, &ack, &ack_value));

    assert_eq!(
        hex32(&compute_root(&env, &leaves)),
        "5ec97edad72860f4a92fb862eba4a16b326f17c3fa3514eb031b4a081dd01391",
        "commitment + receipt + ack"
    );
}

#[test]
fn the_compressed_tree_matches_the_reference_root() {
    let env = Env::default();
    let entries: &[(&[u8], &[u8])] = &[
        (b"a", b"1"),
        (b"b", b"2"),
        (b"c", b"3"),
        (b"10-stellar-0", b"commitment"),
    ];

    assert_eq!(
        compressed_root(&env, entries),
        compute_root(&env, &leaf_set(&env, entries))
    );
}

#[test]
fn the_compressed_tree_matches_the_shared_vectors() {
    let env = Env::default();
    let storage = TestStorage::default();
    let tree = Tree::new(&env, &storage);

    let commitment = v2_path(&env, "10-stellar-0", 0x01, 7);
    let commitment_value = Bytes::from_slice(&env, &[0xABu8; 32]);

    tree.set(
        key_index(&env, &commitment),
        &leaf_hash(&env, &commitment, &commitment_value),
    );

    assert_eq!(
        hex32(&tree.root()),
        "389440afec9968073f33047acc29c80207ba145947177b5bbe46edf836b6e666"
    );

    let receipt = v2_path(&env, "10-stellar-0", 0x02, 7);
    let receipt_value = Bytes::from_slice(&env, &[0x01u8]);
    tree.set(
        key_index(&env, &receipt),
        &leaf_hash(&env, &receipt, &receipt_value),
    );

    let ack = v2_path(&env, "10-stellar-0", 0x03, 7);
    let ack_value = Bytes::from_slice(&env, &[0xCDu8; 32]);
    tree.set(key_index(&env, &ack), &leaf_hash(&env, &ack, &ack_value));

    assert_eq!(
        hex32(&tree.root()),
        "5ec97edad72860f4a92fb862eba4a16b326f17c3fa3514eb031b4a081dd01391"
    );
}

#[test]
fn the_root_changes_with_the_value() {
    let env = Env::default();

    assert_ne!(
        compressed_root(&env, &[(b"k", b"v1")]),
        compressed_root(&env, &[(b"k", b"v2")])
    );
}

#[test]
fn the_root_is_independent_of_insertion_order() {
    let env = Env::default();
    let forward: &[(&[u8], &[u8])] = &[(b"a", b"1"), (b"b", b"2"), (b"c", b"3")];
    let shuffled: &[(&[u8], &[u8])] = &[(b"c", b"3"), (b"a", b"1"), (b"b", b"2")];

    assert_eq!(
        compressed_root(&env, forward),
        compressed_root(&env, shuffled)
    );
}

#[test]
fn overwriting_a_leaf_matches_inserting_it_once() {
    let env = Env::default();
    let storage = TestStorage::default();
    let tree = Tree::new(&env, &storage);

    insert(&env, &tree, b"k", b"first");
    insert(&env, &tree, b"k", b"second");

    assert_eq!(tree.root(), compressed_root(&env, &[(b"k", b"second")]));
}

#[test]
fn a_branch_splits_and_collapses_back() {
    let env = Env::default();
    let storage = TestStorage::default();
    let tree = Tree::new(&env, &storage);

    insert(&env, &tree, b"a", b"1");
    let lone = tree.root();

    insert(&env, &tree, b"b", b"2");
    assert_ne!(tree.root(), lone);

    drop_key(&env, &tree, b"b");

    assert_eq!(
        tree.root(),
        lone,
        "removing the second leaf collapses the branch back to the first"
    );
}

#[test]
fn removing_the_only_leaf_returns_to_the_zero_root() {
    let env = Env::default();
    let storage = TestStorage::default();
    let tree = Tree::new(&env, &storage);

    insert(&env, &tree, b"k", b"v");
    assert_ne!(tree.root(), empty(&env));

    drop_key(&env, &tree, b"k");

    assert_eq!(tree.root(), empty(&env));
}

#[test]
fn a_removed_leaf_can_be_reinserted() {
    let env = Env::default();
    let entries: &[(&[u8], &[u8])] = &[(b"a", b"1"), (b"b", b"2")];
    let expected = compressed_root(&env, entries);

    let storage = TestStorage::default();
    let tree = Tree::new(&env, &storage);

    for (key, value) in entries {
        insert(&env, &tree, key, value);
    }

    drop_key(&env, &tree, b"b");
    insert(&env, &tree, b"b", b"2");

    assert_eq!(tree.root(), expected);
}

#[test]
fn sparse_distant_indices_match_the_reference_root() {
    let env = Env::default();
    let storage = TestStorage::default();
    let tree = Tree::new(&env, &storage);
    let hash = BytesN::from_array(&env, &[0x33; 32]);

    let mut leaves: Map<u64, BytesN<32>> = Map::new(&env);
    for index in [0u64, u64::MAX, 1u64 << 63, 1u64] {
        tree.set(index, &hash);
        leaves.set(index, hash.clone());
    }

    assert_eq!(tree.root(), compute_root(&env, &leaves));
}
