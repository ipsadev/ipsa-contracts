mod commitment;
mod storage;

pub(crate) use commitment::{commit, stored_root};
use ipsa_smt::{key_index, leaf_hash, Tree};
use soroban_sdk::{panic_with_error, Bytes, BytesN, Env};
pub use storage::SmtKey;
use storage::{extend, RouterStorage};

use crate::errors::Error;

pub fn tree_set(env: &Env, index: u64, hash: &BytesN<32>) {
    let storage = RouterStorage::new(env);

    Tree::new(env, &storage).set(index, hash);
}

pub fn tree_remove(env: &Env, index: u64) {
    let storage = RouterStorage::new(env);

    Tree::new(env, &storage).remove(index);
}

pub fn tree_root(env: &Env) -> BytesN<32> {
    let storage = RouterStorage::new(env);

    Tree::new(env, &storage).root()
}

pub fn set_leaf(env: &Env, key: &Bytes, value: &Bytes) {
    let index = key_index(env, key);
    let key_hash = env.crypto().sha256(key).to_bytes();
    let owner_key = SmtKey::Owner(index);

    match env
        .storage()
        .persistent()
        .get::<SmtKey, BytesN<32>>(&owner_key)
    {
        Some(owner) if owner != key_hash => panic_with_error!(env, Error::ProvableSlotTaken),
        Some(_) => {}
        None => env.storage().persistent().set(&owner_key, &key_hash),
    }
    extend(env, &owner_key);

    tree_set(env, index, &leaf_hash(env, key, value));
}

pub fn remove_leaf(env: &Env, key: &Bytes) {
    let index = key_index(env, key);
    let key_hash = env.crypto().sha256(key).to_bytes();
    let storage = env.storage().persistent();

    if !storage.has(&SmtKey::Leaf(index)) {
        return;
    }

    if let Some(owner) = storage.get::<SmtKey, BytesN<32>>(&SmtKey::Owner(index)) {
        if owner != key_hash {
            return;
        }
    }

    storage.remove(&SmtKey::Owner(index));
    tree_remove(env, index);
}
