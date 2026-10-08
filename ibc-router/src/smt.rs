use soroban_sdk::{contracttype, Bytes, BytesN, Env, Map, Vec};

use crate::{
    errors::Error,
    events::IbcRoot,
    ics24::{CONFIG_TTL_EXTEND_TO, CONFIG_TTL_THRESHOLD},
};

pub(crate) const TREE_DEPTH: u32 = 64;

#[contracttype]
#[derive(Clone)]
pub enum SmtKey {
    Root,
    Top,
    Leaf(u64),
    Owner(u64),
    Node(u32, u64),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Child {
    Leaf(u64),
    Node(u32, u64),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub hash: BytesN<32>,
    pub left: Child,
    pub right: Child,
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
    let left = left.to_array();
    let right = right.to_array();

    if left == [0u8; 32] && right == [0u8; 32] {
        return empty(env);
    }

    let mut buf = [0u8; 65];
    buf[0] = 0x01;
    buf[1..33].copy_from_slice(&left);
    buf[33..].copy_from_slice(&right);

    env.crypto()
        .sha256(&Bytes::from_array(env, &buf))
        .to_bytes()
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

fn extend(env: &Env, key: &SmtKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
}

fn position(child: &Child) -> (u32, u64) {
    match child {
        Child::Leaf(index) => (0, *index),
        Child::Node(level, prefix) => (*level, *prefix),
    }
}

fn load_branch(env: &Env, level: u32, prefix: u64) -> Branch {
    let key = SmtKey::Node(level, prefix);
    let branch = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic_with_error(env, Error::ProvableStoreCorrupt));
    extend(env, &key);

    branch
}

fn save_branch(env: &Env, level: u32, prefix: u64, branch: &Branch) {
    let key = SmtKey::Node(level, prefix);
    env.storage().persistent().set(&key, branch);
    extend(env, &key);
}

fn child_hash(env: &Env, child: &Child) -> BytesN<32> {
    match child {
        Child::Leaf(index) => env
            .storage()
            .persistent()
            .get(&SmtKey::Leaf(*index))
            .unwrap_or_else(|| panic_with_error(env, Error::ProvableStoreCorrupt)),
        Child::Node(level, prefix) => load_branch(env, *level, *prefix).hash,
    }
}

fn lift(env: &Env, hash: BytesN<32>, from: u32, prefix: u64, to: u32) -> BytesN<32> {
    let empty = empty(env);
    let mut hash = hash;

    for level in from..to {
        let at_level = prefix >> (level - from);
        hash = if at_level & 1 == 0 {
            inner_hash(env, &hash, &empty)
        } else {
            inner_hash(env, &empty, &hash)
        };
    }

    hash
}

fn lifted_child(env: &Env, child: &Child, to: u32) -> BytesN<32> {
    let (level, prefix) = position(child);

    lift(env, child_hash(env, child), level, prefix, to)
}

fn branch_hash(env: &Env, level: u32, left: &Child, right: &Child) -> BytesN<32> {
    inner_hash(
        env,
        &lifted_child(env, left, level - 1),
        &lifted_child(env, right, level - 1),
    )
}

fn load_top(env: &Env) -> Option<Child> {
    env.storage().persistent().get(&SmtKey::Top)
}

fn save_top(env: &Env, top: Option<Child>) {
    let storage = env.storage().persistent();

    match top {
        Some(child) => {
            storage.set(&SmtKey::Top, &child);
            extend(env, &SmtKey::Top);
        }
        None => storage.remove(&SmtKey::Top),
    }
}

fn contains(child: &Child, index: u64) -> bool {
    let (level, prefix) = position(child);

    if level >= TREE_DEPTH {
        return true;
    }

    index >> level == prefix
}

fn descend(env: &Env, index: u64) -> (Vec<(u32, u64)>, Option<Child>) {
    let mut path = Vec::new(env);
    let mut current = load_top(env);

    while let Some(child) = current.clone() {
        let Child::Node(level, prefix) = child else {
            break;
        };

        if !contains(&child, index) {
            break;
        }

        let branch = load_branch(env, level, prefix);
        path.push_back((level, prefix));
        current = Some(if (index >> (level - 1)) & 1 == 0 {
            branch.left
        } else {
            branch.right
        });
    }

    (path, current)
}

fn replace_child(env: &Env, path: &Vec<(u32, u64)>, old: &Child, new: Option<Child>) {
    let Some((level, prefix)) = path.last() else {
        save_top(env, new);

        return;
    };

    let mut branch = load_branch(env, level, prefix);
    let replacement = new.unwrap_or_else(|| panic_with_error(env, Error::ProvableStoreCorrupt));

    if branch.left == *old {
        branch.left = replacement;
    } else if branch.right == *old {
        branch.right = replacement;
    } else {
        panic_with_error(env, Error::ProvableStoreCorrupt);
    }

    save_branch(env, level, prefix, &branch);
}

fn rehash(env: &Env, path: &Vec<(u32, u64)>) {
    for (level, prefix) in path.iter().rev() {
        let mut branch = load_branch(env, level, prefix);
        branch.hash = branch_hash(env, level, &branch.left, &branch.right);
        save_branch(env, level, prefix, &branch);
    }
}

fn split_level(index: u64, existing: &Child) -> u32 {
    let (level, prefix) = position(existing);
    let base = prefix.checked_shl(level).unwrap_or(0);
    let differing = base ^ index;

    TREE_DEPTH - differing.leading_zeros()
}

pub fn tree_set(env: &Env, index: u64, hash: &BytesN<32>) {
    let leaf_key = SmtKey::Leaf(index);
    let existed = env.storage().persistent().has(&leaf_key);

    env.storage().persistent().set(&leaf_key, hash);
    extend(env, &leaf_key);

    let (path, reached) = descend(env, index);

    if existed {
        rehash(env, &path);

        return;
    }

    let leaf = Child::Leaf(index);

    let Some(existing) = reached else {
        save_top(env, Some(leaf));

        return;
    };

    let level = split_level(index, &existing);
    let prefix = index.checked_shr(level).unwrap_or(0);
    let (left, right) = if (index >> (level - 1)) & 1 == 0 {
        (leaf, existing.clone())
    } else {
        (existing.clone(), leaf)
    };
    let branch = Branch {
        hash: branch_hash(env, level, &left, &right),
        left,
        right,
    };

    save_branch(env, level, prefix, &branch);
    replace_child(env, &path, &existing, Some(Child::Node(level, prefix)));
    rehash(env, &path);
}

pub fn tree_remove(env: &Env, index: u64) {
    let leaf_key = SmtKey::Leaf(index);

    if !env.storage().persistent().has(&leaf_key) {
        return;
    }

    env.storage().persistent().remove(&leaf_key);

    let (mut path, reached) = descend(env, index);

    if reached != Some(Child::Leaf(index)) {
        panic_with_error(env, Error::ProvableStoreCorrupt);
    }

    let Some((level, prefix)) = path.pop_back() else {
        save_top(env, None);

        return;
    };

    let parent = load_branch(env, level, prefix);
    let sibling = if parent.left == Child::Leaf(index) {
        parent.right
    } else {
        parent.left
    };

    env.storage()
        .persistent()
        .remove(&SmtKey::Node(level, prefix));
    replace_child(env, &path, &Child::Node(level, prefix), Some(sibling));
    rehash(env, &path);
}

pub fn tree_root(env: &Env) -> BytesN<32> {
    match load_top(env) {
        None => empty(env),
        Some(child) => lifted_child(env, &child, TREE_DEPTH),
    }
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
        Some(owner) if owner != key_hash => panic_with_error(env, Error::ProvableSlotTaken),
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

fn panic_with_error(env: &Env, error: Error) -> ! {
    soroban_sdk::panic_with_error!(env, error)
}

pub(crate) fn commit(env: &Env) -> BytesN<32> {
    let root = tree_root(env);
    let storage = env.storage().persistent();
    storage.set(&SmtKey::Root, &root);
    extend(env, &SmtKey::Root);
    if storage.has(&SmtKey::Top) {
        extend(env, &SmtKey::Top);
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
