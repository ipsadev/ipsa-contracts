use soroban_sdk::{contracttype, Bytes, BytesN, Env, Map};

use crate::errors::Error;
use crate::events::IbcRoot;
use crate::ics24::{CONFIG_TTL_EXTEND_TO, CONFIG_TTL_THRESHOLD};

pub(crate) const TREE_DEPTH: u32 = 64;

pub(crate) const MAX_LEAVES: u32 = 512;

#[contracttype]
#[derive(Clone)]
pub enum SmtKey {
    Leaves,
    Root,
}

fn empty(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

fn is_empty(hash: &BytesN<32>) -> bool {
    hash.to_array() == [0u8; 32]
}

pub(crate) fn key_index(env: &Env, key: &Bytes) -> u64 {
    let digest = env.crypto().sha256(key).to_bytes().to_array();
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(head)
}

fn leaf_hash(env: &Env, key: &Bytes, value: &Bytes) -> BytesN<32> {
    let key_hash: Bytes = env.crypto().sha256(key).to_bytes().into();
    let value_hash: Bytes = env.crypto().sha256(value).to_bytes().into();

    let mut buf = Bytes::from_slice(env, &[0x00]);
    buf.append(&key_hash);
    buf.append(&value_hash);
    env.crypto().sha256(&buf).to_bytes()
}

fn inner_hash(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
    if is_empty(left) && is_empty(right) {
        return empty(env);
    }
    let mut buf = Bytes::from_slice(env, &[0x01]);
    buf.append(&Bytes::from(left.clone()));
    buf.append(&Bytes::from(right.clone()));
    env.crypto().sha256(&buf).to_bytes()
}

fn load_leaves(env: &Env) -> Map<u64, BytesN<32>> {
    env.storage()
        .persistent()
        .get(&SmtKey::Leaves)
        .unwrap_or_else(|| Map::new(env))
}

fn save_leaves(env: &Env, leaves: &Map<u64, BytesN<32>>) {
    env.storage().persistent().set(&SmtKey::Leaves, leaves);
}

pub(crate) fn set_leaf(env: &Env, key: &Bytes, value: &Bytes) {
    let mut leaves = load_leaves(env);
    let idx = key_index(env, key);
    if leaves.get(idx).is_none() && leaves.len() >= MAX_LEAVES {
        panic_with_error(env);
    }
    leaves.set(idx, leaf_hash(env, key, value));
    save_leaves(env, &leaves);
}

pub(crate) fn remove_leaf(env: &Env, key: &Bytes) {
    let mut leaves = load_leaves(env);
    let idx = key_index(env, key);
    if leaves.get(idx).is_none() {
        return;
    }
    leaves.remove(idx);
    save_leaves(env, &leaves);
}

fn panic_with_error(env: &Env) -> ! {
    soroban_sdk::panic_with_error!(env, Error::ProvableStoreFull)
}

pub(crate) fn commit(env: &Env) -> BytesN<32> {
    let root = compute_root(env, &load_leaves(env));
    let storage = env.storage().persistent();
    storage.set(&SmtKey::Root, &root);
    storage.extend_ttl(&SmtKey::Root, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
    if storage.has(&SmtKey::Leaves) {
        storage.extend_ttl(&SmtKey::Leaves, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
    }
    IbcRoot { root: root.clone() }.publish(env);
    root
}

pub(crate) fn stored_root(env: &Env) -> BytesN<32> {
    env.storage()
        .persistent()
        .get(&SmtKey::Root)
        .unwrap_or_else(|| empty(env))
}

pub(crate) fn compute_root(env: &Env, leaves: &Map<u64, BytesN<32>>) -> BytesN<32> {
    if leaves.is_empty() {
        return empty(env);
    }

    let mut level = leaves.clone();
    for _ in 0..TREE_DEPTH {
        let mut next: Map<u64, BytesN<32>> = Map::new(env);
        for (idx, _) in level.iter() {
            let parent = idx >> 1;
            if next.contains_key(parent) {
                continue;
            }
            let left = level.get(parent << 1).unwrap_or_else(|| empty(env));
            let right = level.get((parent << 1) | 1).unwrap_or_else(|| empty(env));
            let folded = inner_hash(env, &left, &right);
            if !is_empty(&folded) {
                next.set(parent, folded);
            }
        }
        level = next;
    }

    level.get(0).unwrap_or_else(|| empty(env))
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use soroban_sdk::Env;

    fn leaf_set(env: &Env, entries: &[(&[u8], &[u8])]) -> Map<u64, BytesN<32>> {
        let mut leaves: Map<u64, BytesN<32>> = Map::new(env);
        for (k, v) in entries {
            let key = Bytes::from_slice(env, k);
            let value = Bytes::from_slice(env, v);
            leaves.set(key_index(env, &key), leaf_hash(env, &key, &value));
        }
        leaves
    }

    #[test]
    fn an_empty_tree_has_a_zero_root() {
        let env = Env::default();
        assert_eq!(compute_root(&env, &Map::new(&env)), empty(&env));
    }

    #[test]
    fn empty_subtrees_collapse_to_zero() {
        let env = Env::default();
        let e = empty(&env);
        assert_eq!(inner_hash(&env, &e, &e), e);
        assert_ne!(
            inner_hash(&env, &BytesN::from_array(&env, &[1u8; 32]), &e),
            BytesN::from_array(&env, &[1u8; 32]),
            "a lone child must still be folded, not passed through"
        );
    }

    #[test]
    fn the_root_changes_with_the_value() {
        let env = Env::default();
        let a = compute_root(&env, &leaf_set(&env, &[(b"k", b"v1")]));
        let b = compute_root(&env, &leaf_set(&env, &[(b"k", b"v2")]));
        assert_ne!(a, b);
    }

    #[test]
    fn the_root_is_independent_of_insertion_order() {
        let env = Env::default();
        let a = leaf_set(&env, &[(b"a", b"1"), (b"b", b"2"), (b"c", b"3")]);
        let b = leaf_set(&env, &[(b"c", b"3"), (b"a", b"1"), (b"b", b"2")]);
        assert_eq!(compute_root(&env, &a), compute_root(&env, &b));
    }

    #[test]
    fn removing_the_only_leaf_returns_to_the_zero_root() {
        let env = Env::default();
        let full = leaf_set(&env, &[(b"k", b"v")]);
        assert_ne!(compute_root(&env, &full), empty(&env));
        assert_eq!(compute_root(&env, &Map::new(&env)), empty(&env));
    }

    fn v2_path(env: &Env, client: &str, discriminator: u8, sequence: u64) -> Bytes {
        let mut key = client.as_bytes().to_vec();
        key.push(discriminator);
        key.extend_from_slice(&sequence.to_be_bytes());
        Bytes::from_slice(env, &key)
    }

    fn hex32(hash: &BytesN<32>) -> alloc::string::String {
        use core::fmt::Write;
        let mut out = alloc::string::String::new();
        for b in hash.to_array() {
            let _ = write!(out, "{b:02x}");
        }
        out
    }

    #[test]
    fn shared_smt_vectors() {
        let env = Env::default();

        let mut leaves: Map<u64, BytesN<32>> = Map::new(&env);
        let commitment = v2_path(&env, "10-stellar-0", 0x01, 7);
        let commitment_value = Bytes::from_slice(&env, &[0xABu8; 32]);
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
    fn leaf_hash_is_sha256_of_the_tagged_pair() {
        let env = Env::default();
        let key = Bytes::from_slice(&env, b"path");
        let value = Bytes::from_slice(&env, b"value");

        let mut expected = Bytes::from_slice(&env, &[0x00]);
        expected.append(&env.crypto().sha256(&key).to_bytes().into());
        expected.append(&env.crypto().sha256(&value).to_bytes().into());

        assert_eq!(
            leaf_hash(&env, &key, &value),
            env.crypto().sha256(&expected).to_bytes()
        );
    }
}
