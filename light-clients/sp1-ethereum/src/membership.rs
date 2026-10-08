use soroban_sdk::{Bytes, BytesN, Env};

pub const OUTPUT_LEN: u32 = 104;

const CLIENT_STATE_COMMITMENT: u32 = 0;
const CONSENSUS_HASH: u32 = 32;
const SLOT: u32 = 64;
const KV_COMMITMENT: u32 = 72;

pub struct Output {
    pub client_state_commitment: BytesN<32>,
    pub consensus_hash: BytesN<32>,
    pub slot: u64,
    pub kv_commitment: BytesN<32>,
}

pub fn decode(env: &Env, public_values: &Bytes) -> Output {
    if public_values.len() != OUTPUT_LEN {
        panic!("the committed public values are not the expected length");
    }

    Output {
        client_state_commitment: hash_at(env, public_values, CLIENT_STATE_COMMITMENT),
        consensus_hash: hash_at(env, public_values, CONSENSUS_HASH),
        slot: u64_at(public_values, SLOT),
        kv_commitment: hash_at(env, public_values, KV_COMMITMENT),
    }
}

pub fn kv_commitment(env: &Env, path: &Bytes, value: &Bytes) -> BytesN<32> {
    let mut preimage = Bytes::new(env);

    preimage.extend_from_array(&1_u32.to_be_bytes());
    preimage.extend_from_array(&path.len().to_be_bytes());
    preimage.append(path);
    preimage.extend_from_array(&value.len().to_be_bytes());
    preimage.append(value);

    env.crypto().sha256(&preimage).to_bytes()
}

fn hash_at(env: &Env, source: &Bytes, offset: u32) -> BytesN<32> {
    let mut raw = [0_u8; 32];

    for index in 0..32_u32 {
        raw[index as usize] = source.get(offset + index).unwrap_or(0);
    }

    BytesN::from_array(env, &raw)
}

fn u64_at(source: &Bytes, offset: u32) -> u64 {
    let mut raw = [0_u8; 8];

    for index in 0..8_u32 {
        raw[index as usize] = source.get(offset + index).unwrap_or(0);
    }

    u64::from_be_bytes(raw)
}
