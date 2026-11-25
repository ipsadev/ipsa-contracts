use soroban_sdk::{Bytes, BytesN, Env};

pub const TREE_DEPTH: u32 = 64;

pub fn empty(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

pub fn is_empty(hash: &BytesN<32>) -> bool {
    hash.to_array() == [0u8; 32]
}

pub fn key_index(env: &Env, key: &Bytes) -> u64 {
    let digest = env.crypto().sha256(key).to_array();

    u64::from_be_bytes(digest[..8].try_into().unwrap())
}

pub fn leaf_hash(env: &Env, key: &Bytes, value: &Bytes) -> BytesN<32> {
    let mut buffer = Bytes::from_slice(env, &[0x00]);

    buffer.append(&env.crypto().sha256(key).to_bytes().into());
    buffer.append(&env.crypto().sha256(value).to_bytes().into());

    env.crypto().sha256(&buffer).to_bytes()
}

pub fn inner_hash(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
    if is_empty(left) && is_empty(right) {
        return empty(env);
    }

    let mut buffer = [0u8; 65];

    buffer[0] = 0x01;
    buffer[1..33].copy_from_slice(&left.to_array());
    buffer[33..].copy_from_slice(&right.to_array());

    env.crypto()
        .sha256(&Bytes::from_array(env, &buffer))
        .to_bytes()
}
