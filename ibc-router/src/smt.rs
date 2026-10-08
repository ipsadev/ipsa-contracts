use soroban_sdk::{contracttype, Bytes, BytesN, Env, Map};

use crate::{
    errors::Error,
    events::IbcRoot,
    ics24::{CONFIG_TTL_EXTEND_TO, CONFIG_TTL_THRESHOLD},
};

pub(crate) const TREE_DEPTH: u32 = 64;

pub(crate) const MAX_LEAVES: u32 = 512;

#[contracttype]
#[derive(Clone)]
pub enum SmtKey {
    Leaves,
    Root,
    Owners,
}

pub fn empty(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

fn is_empty(hash: &BytesN<32>) -> bool {
    hash.to_array() == [0u8; 32]
}

pub fn key_index(env: &Env, key: &Bytes) -> u64 {
    let digest = env.crypto().sha256(key).to_bytes().to_array();
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(head)
}

pub fn leaf_hash(env: &Env, key: &Bytes, value: &Bytes) -> BytesN<32> {
    let key_hash: Bytes = env.crypto().sha256(key).to_bytes().into();
    let value_hash: Bytes = env.crypto().sha256(value).to_bytes().into();

    let mut buf = Bytes::from_slice(env, &[0x00]);
    buf.append(&key_hash);
    buf.append(&value_hash);
    env.crypto().sha256(&buf).to_bytes()
}

pub fn inner_hash(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
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

fn load_owners(env: &Env) -> Map<u64, BytesN<32>> {
    env.storage()
        .persistent()
        .get(&SmtKey::Owners)
        .unwrap_or_else(|| Map::new(env))
}

fn save_owners(env: &Env, owners: &Map<u64, BytesN<32>>) {
    env.storage().persistent().set(&SmtKey::Owners, owners);
}

pub fn claim_slot(owners: &mut Map<u64, BytesN<32>>, index: u64, key_hash: &BytesN<32>) -> bool {
    match owners.get(index) {
        Some(owner) => owner == *key_hash,
        None => {
            owners.set(index, key_hash.clone());

            true
        }
    }
}

pub fn release_slot(
    leaves: &mut Map<u64, BytesN<32>>,
    owners: &mut Map<u64, BytesN<32>>,
    index: u64,
    key_hash: &BytesN<32>,
) -> bool {
    if !leaves.contains_key(index) {
        return false;
    }

    if let Some(owner) = owners.get(index) {
        if owner != *key_hash {
            return false;
        }
    }

    leaves.remove(index);
    owners.remove(index);

    true
}

pub(crate) fn set_leaf(env: &Env, key: &Bytes, value: &Bytes) {
    let mut leaves = load_leaves(env);
    let mut owners = load_owners(env);
    let idx = key_index(env, key);
    let key_hash = env.crypto().sha256(key).to_bytes();

    if leaves.get(idx).is_none() && leaves.len() >= MAX_LEAVES {
        panic_with_error(env, Error::ProvableStoreFull);
    }

    if !claim_slot(&mut owners, idx, &key_hash) {
        panic_with_error(env, Error::ProvableSlotTaken);
    }

    leaves.set(idx, leaf_hash(env, key, value));
    save_leaves(env, &leaves);
    save_owners(env, &owners);
}

pub(crate) fn remove_leaf(env: &Env, key: &Bytes) {
    let mut leaves = load_leaves(env);
    let mut owners = load_owners(env);
    let idx = key_index(env, key);
    let key_hash = env.crypto().sha256(key).to_bytes();

    if !release_slot(&mut leaves, &mut owners, idx, &key_hash) {
        return;
    }

    save_leaves(env, &leaves);
    save_owners(env, &owners);
}

fn panic_with_error(env: &Env, error: Error) -> ! {
    soroban_sdk::panic_with_error!(env, error)
}

pub(crate) fn commit(env: &Env) -> BytesN<32> {
    let root = compute_root(env, &load_leaves(env));
    let storage = env.storage().persistent();
    storage.set(&SmtKey::Root, &root);
    storage.extend_ttl(&SmtKey::Root, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
    if storage.has(&SmtKey::Leaves) {
        storage.extend_ttl(&SmtKey::Leaves, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
    }
    if storage.has(&SmtKey::Owners) {
        storage.extend_ttl(&SmtKey::Owners, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
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

pub fn compute_root(env: &Env, leaves: &Map<u64, BytesN<32>>) -> BytesN<32> {
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
