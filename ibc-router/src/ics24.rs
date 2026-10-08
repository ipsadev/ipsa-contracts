#![allow(dead_code)]

use soroban_sdk::{Bytes, BytesN, Env, String};

use crate::types::{Packet, Payload};

pub const PACKET_COMMITMENT_DISCRIMINATOR: u8 = 0x01;
pub const PACKET_RECEIPT_DISCRIMINATOR: u8 = 0x02;
pub const ACK_COMMITMENT_DISCRIMINATOR: u8 = 0x03;

pub(crate) const RECEIPT_SENTINEL: u8 = 0x01;
pub(crate) const COMMITMENT_VERSION_PREFIX: u8 = 0x02;

pub(crate) const PROVABLE_TTL_THRESHOLD: u32 = 17_280;
pub(crate) const PROVABLE_TTL_EXTEND_TO: u32 = 86_400;

pub(crate) const CONFIG_TTL_THRESHOLD: u32 = 86_400;
pub(crate) const CONFIG_TTL_EXTEND_TO: u32 = 518_400;

pub(crate) const ERROR_ACK_PREIMAGE: &[u8] = b"UNIVERSAL_ERROR_ACKNOWLEDGEMENT";

pub fn packet_commitment_path(env: &Env, source_client_id: &String, sequence: u64) -> Bytes {
    v2_path_key(
        env,
        source_client_id,
        PACKET_COMMITMENT_DISCRIMINATOR,
        sequence,
    )
}

pub fn packet_receipt_path(env: &Env, dest_client_id: &String, sequence: u64) -> Bytes {
    v2_path_key(env, dest_client_id, PACKET_RECEIPT_DISCRIMINATOR, sequence)
}

pub fn ack_commitment_path(env: &Env, dest_client_id: &String, sequence: u64) -> Bytes {
    v2_path_key(env, dest_client_id, ACK_COMMITMENT_DISCRIMINATOR, sequence)
}

pub(crate) fn v2_path_key(
    env: &Env,
    client_id: &String,
    discriminator: u8,
    sequence: u64,
) -> Bytes {
    let id_len = client_id.len() as usize;
    let mut buf = [0u8; 128];
    client_id.copy_into_slice(&mut buf[..id_len]);
    buf[id_len] = discriminator;
    let seq_bytes = sequence.to_be_bytes();
    buf[id_len + 1..id_len + 9].copy_from_slice(&seq_bytes);
    Bytes::from_slice(env, &buf[..id_len + 9])
}

pub fn error_ack_hash(env: &Env) -> BytesN<32> {
    sha256_bytes(env, &Bytes::from_slice(env, ERROR_ACK_PREIMAGE))
}

pub(crate) fn commit_payload(env: &Env, payload: &Payload) -> BytesN<32> {
    let mut buf = Bytes::new(env);
    buf.append(&sha256_string(env, &payload.source_port).into());
    buf.append(&sha256_string(env, &payload.dest_port).into());
    buf.append(&sha256_string(env, &payload.version).into());
    buf.append(&sha256_string(env, &payload.encoding).into());
    buf.append(&sha256_bytes(env, &payload.value).into());
    sha256_bytes(env, &buf)
}

pub fn commit_v2_packet(env: &Env, packet: &Packet) -> BytesN<32> {
    let mut payloads_concat = Bytes::new(env);
    for p in packet.payloads.iter() {
        let payload_hash: Bytes = commit_payload(env, &p).into();
        payloads_concat.append(&payload_hash);
    }

    let timeout_be = packet.timeout_timestamp.to_be_bytes();
    let timeout_bytes = Bytes::from_slice(env, &timeout_be);

    let mut preimage = Bytes::new(env);
    preimage.append(&Bytes::from_slice(env, &[COMMITMENT_VERSION_PREFIX]));
    preimage.append(&sha256_string(env, &packet.dest_client).into());
    preimage.append(&sha256_bytes(env, &timeout_bytes).into());
    preimage.append(&sha256_bytes(env, &payloads_concat).into());

    sha256_bytes(env, &preimage)
}

pub fn commit_v2_acknowledgement(env: &Env, app_acks: &soroban_sdk::Vec<Bytes>) -> BytesN<32> {
    let mut concat = Bytes::new(env);
    for ack in app_acks.iter() {
        concat.append(&sha256_bytes(env, &ack).into());
    }

    let mut preimage = Bytes::new(env);
    preimage.append(&Bytes::from_slice(env, &[COMMITMENT_VERSION_PREFIX]));
    preimage.append(&concat);

    sha256_bytes(env, &preimage)
}

fn sha256_bytes(env: &Env, data: &Bytes) -> BytesN<32> {
    env.crypto().sha256(data).to_bytes()
}

fn sha256_string(env: &Env, s: &String) -> BytesN<32> {
    let len = s.len() as usize;
    let mut buf = [0u8; 256];
    s.copy_into_slice(&mut buf[..len]);
    sha256_bytes(env, &Bytes::from_slice(env, &buf[..len]))
}
