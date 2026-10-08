use soroban_sdk::{Bytes, BytesN, Env};

pub const OUTPUT_LEN: u32 = 120;

const CLIENT_STATE_COMMITMENT: u32 = 0;
const TRUSTED_CONSENSUS_HASH: u32 = 32;
const NEW_CONSENSUS_HASH: u32 = 64;
const TRUSTED_SLOT: u32 = 96;
const NEW_SLOT: u32 = 104;
const TIMESTAMP: u32 = 112;

pub struct Output {
    pub client_state_commitment: BytesN<32>,
    pub trusted_consensus_hash: BytesN<32>,
    pub new_consensus_hash: BytesN<32>,
    pub trusted_slot: u64,
    pub new_slot: u64,
    pub timestamp: u64,
}

pub fn decode(env: &Env, public_values: &Bytes) -> Output {
    if public_values.len() != OUTPUT_LEN {
        panic!("the committed public values are not the expected length");
    }

    Output {
        client_state_commitment: hash_at(env, public_values, CLIENT_STATE_COMMITMENT),
        trusted_consensus_hash: hash_at(env, public_values, TRUSTED_CONSENSUS_HASH),
        new_consensus_hash: hash_at(env, public_values, NEW_CONSENSUS_HASH),
        trusted_slot: u64_at(public_values, TRUSTED_SLOT),
        new_slot: u64_at(public_values, NEW_SLOT),
        timestamp: u64_at(public_values, TIMESTAMP),
    }
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
