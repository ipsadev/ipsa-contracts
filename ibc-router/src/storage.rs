use soroban_sdk::{contractimpl, Bytes, BytesN, Env, String};

use crate::{
    ics24::{
        ack_commitment_path, packet_commitment_path, packet_receipt_path, CONFIG_TTL_EXTEND_TO,
        CONFIG_TTL_THRESHOLD, PROVABLE_TTL_EXTEND_TO, PROVABLE_TTL_THRESHOLD, RECEIPT_SENTINEL,
    },
    smt,
    types::DataKey,
    IbcRouter, IbcRouterArgs, IbcRouterClient,
};

pub trait Store {
    fn packet_commitment(env: &Env, source_client_id: String, sequence: u64) -> Option<BytesN<32>>;

    fn has_packet_receipt(env: &Env, dest_client_id: String, sequence: u64) -> bool;

    fn acknowledgement(env: &Env, dest_client_id: String, sequence: u64) -> Option<BytesN<32>>;
}

#[contractimpl]
impl Store for IbcRouter {
    fn packet_commitment(env: &Env, source_client_id: String, sequence: u64) -> Option<BytesN<32>> {
        packet_commitment(env, &source_client_id, sequence)
    }

    fn has_packet_receipt(env: &Env, dest_client_id: String, sequence: u64) -> bool {
        has_packet_receipt(env, &dest_client_id, sequence)
    }

    fn acknowledgement(env: &Env, dest_client_id: String, sequence: u64) -> Option<BytesN<32>> {
        acknowledgement(env, &dest_client_id, sequence)
    }
}

pub(crate) fn set_packet_commitment(
    env: &Env,
    source_client_id: &String,
    sequence: u64,
    commitment: &BytesN<32>,
) {
    let key = packet_commitment_path(env, source_client_id, sequence);
    let storage = env.storage().persistent();
    storage.set(&key, commitment);
    storage.extend_ttl(&key, PROVABLE_TTL_THRESHOLD, PROVABLE_TTL_EXTEND_TO);
    smt::set_leaf(env, &key, &Bytes::from(commitment.clone()));
}

pub(crate) fn packet_commitment(
    env: &Env,
    source_client_id: &String,
    sequence: u64,
) -> Option<BytesN<32>> {
    let key = packet_commitment_path(env, source_client_id, sequence);
    env.storage().persistent().get(&key)
}

pub(crate) fn delete_packet_commitment(env: &Env, source_client_id: &String, sequence: u64) {
    let key = packet_commitment_path(env, source_client_id, sequence);
    env.storage().persistent().remove(&key);
    smt::remove_leaf(env, &key);
}

pub(crate) fn set_packet_receipt(env: &Env, dest_client_id: &String, sequence: u64) {
    let key = packet_receipt_path(env, dest_client_id, sequence);
    let sentinel = Bytes::from_slice(env, &[RECEIPT_SENTINEL]);
    let storage = env.storage().persistent();
    storage.set(&key, &sentinel);
    storage.extend_ttl(&key, PROVABLE_TTL_THRESHOLD, PROVABLE_TTL_EXTEND_TO);
    smt::set_leaf(env, &key, &sentinel);
}

pub(crate) fn has_packet_receipt(env: &Env, dest_client_id: &String, sequence: u64) -> bool {
    let key = packet_receipt_path(env, dest_client_id, sequence);
    env.storage().persistent().has(&key)
}

pub(crate) fn set_ack_commitment(
    env: &Env,
    dest_client_id: &String,
    sequence: u64,
    ack_hash: &BytesN<32>,
) {
    let key = ack_commitment_path(env, dest_client_id, sequence);
    let storage = env.storage().persistent();
    storage.set(&key, ack_hash);
    storage.extend_ttl(&key, PROVABLE_TTL_THRESHOLD, PROVABLE_TTL_EXTEND_TO);
    smt::set_leaf(env, &key, &Bytes::from(ack_hash.clone()));
}

pub(crate) fn acknowledgement(
    env: &Env,
    dest_client_id: &String,
    sequence: u64,
) -> Option<BytesN<32>> {
    let key = ack_commitment_path(env, dest_client_id, sequence);
    env.storage().persistent().get(&key)
}

pub(crate) fn extend_config(env: &Env, key: &DataKey) {
    let storage = env.storage().persistent();
    if storage.has(key) {
        storage.extend_ttl(key, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
    }
}
