#![allow(dead_code)]

use soroban_sdk::{Bytes, BytesN, Env, String};

use crate::types::{Packet, Payload};

pub(crate) const PACKET_COMMITMENT_DISCRIMINATOR: u8 = 0x01;
pub(crate) const PACKET_RECEIPT_DISCRIMINATOR: u8 = 0x02;
pub(crate) const ACK_COMMITMENT_DISCRIMINATOR: u8 = 0x03;

pub(crate) const RECEIPT_SENTINEL: u8 = 0x01;
pub(crate) const COMMITMENT_VERSION_PREFIX: u8 = 0x02;

pub(crate) const PROVABLE_TTL_THRESHOLD: u32 = 17_280;
pub(crate) const PROVABLE_TTL_EXTEND_TO: u32 = 86_400;

pub(crate) const CONFIG_TTL_THRESHOLD: u32 = 86_400;
pub(crate) const CONFIG_TTL_EXTEND_TO: u32 = 518_400;

pub(crate) const ERROR_ACK_PREIMAGE: &[u8] = b"UNIVERSAL_ERROR_ACKNOWLEDGEMENT";

pub(crate) fn packet_commitment_path(env: &Env, source_client_id: &String, sequence: u64) -> Bytes {
    v2_path_key(
        env,
        source_client_id,
        PACKET_COMMITMENT_DISCRIMINATOR,
        sequence,
    )
}

pub(crate) fn packet_receipt_path(env: &Env, dest_client_id: &String, sequence: u64) -> Bytes {
    v2_path_key(env, dest_client_id, PACKET_RECEIPT_DISCRIMINATOR, sequence)
}

pub(crate) fn ack_commitment_path(env: &Env, dest_client_id: &String, sequence: u64) -> Bytes {
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

pub(crate) fn error_ack_hash(env: &Env) -> BytesN<32> {
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

pub(crate) fn commit_v2_packet(env: &Env, packet: &Packet) -> BytesN<32> {
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

pub(crate) fn commit_v2_acknowledgement(
    env: &Env,
    app_acks: &soroban_sdk::Vec<Bytes>,
) -> BytesN<32> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{vec, Env};

    fn test_env() -> Env {
        Env::default()
    }

    #[test]
    fn discriminators_match_ics24_spec() {
        assert_eq!(PACKET_COMMITMENT_DISCRIMINATOR, 0x01);
        assert_eq!(PACKET_RECEIPT_DISCRIMINATOR, 0x02);
        assert_eq!(ACK_COMMITMENT_DISCRIMINATOR, 0x03);
    }

    #[test]
    fn paths_have_clientid_disc_be64_layout() {
        let env = test_env();
        let client_id = String::from_str(&env, "10-stellar-0");
        let path = packet_commitment_path(&env, &client_id, 0x1234);

        let mut actual = [0u8; 32];
        path.copy_into_slice(&mut actual[..path.len() as usize]);
        assert_eq!(&actual[..12], b"10-stellar-0");
        assert_eq!(actual[12], 0x01);
        assert_eq!(&actual[13..21], &0x1234u64.to_be_bytes());
        assert_eq!(path.len(), 21);
    }

    #[test]
    fn receipt_and_ack_use_distinct_discriminators() {
        let env = test_env();
        let id = String::from_str(&env, "x");
        let receipt = packet_receipt_path(&env, &id, 0);
        let ack = ack_commitment_path(&env, &id, 0);

        let mut r = [0u8; 16];
        let mut a = [0u8; 16];
        receipt.copy_into_slice(&mut r[..receipt.len() as usize]);
        ack.copy_into_slice(&mut a[..ack.len() as usize]);
        assert_eq!(r[1], 0x02);
        assert_eq!(a[1], 0x03);
    }

    #[test]
    fn error_ack_hash_is_deterministic_and_nonzero() {
        let env = test_env();
        let h1 = error_ack_hash(&env);
        let h2 = error_ack_hash(&env);
        assert_eq!(h1, h2);
        assert_ne!(h1, BytesN::from_array(&env, &[0u8; 32]));
    }

    #[test]
    fn commit_v2_packet_is_deterministic() {
        let env = test_env();
        let payload = Payload {
            source_port: String::from_str(&env, "transfer"),
            dest_port: String::from_str(&env, "transfer"),
            version: String::from_str(&env, "v1"),
            encoding: String::from_str(&env, "json"),
            value: Bytes::from_slice(&env, b"hello"),
        };
        let packet = Packet {
            sequence: 1,
            source_client: String::from_str(&env, "10-stellar-0"),
            dest_client: String::from_str(&env, "07-tendermint-0"),
            timeout_timestamp: 1_000,
            payloads: vec![&env, payload],
        };

        let h1 = commit_v2_packet(&env, &packet);
        let h2 = commit_v2_packet(&env, &packet);
        assert_eq!(h1, h2);
    }

    #[test]
    fn commit_v2_packet_changes_with_inputs() {
        let env = test_env();
        let payload = Payload {
            source_port: String::from_str(&env, "transfer"),
            dest_port: String::from_str(&env, "transfer"),
            version: String::from_str(&env, "v1"),
            encoding: String::from_str(&env, "json"),
            value: Bytes::from_slice(&env, b"hello"),
        };
        let mut packet = Packet {
            sequence: 1,
            source_client: String::from_str(&env, "10-stellar-0"),
            dest_client: String::from_str(&env, "07-tendermint-0"),
            timeout_timestamp: 1_000,
            payloads: vec![&env, payload.clone()],
        };

        let base = commit_v2_packet(&env, &packet);

        packet.timeout_timestamp = 2_000;
        assert_ne!(
            base,
            commit_v2_packet(&env, &packet),
            "timeout must affect commitment"
        );

        packet.timeout_timestamp = 1_000;
        packet.dest_client = String::from_str(&env, "07-tendermint-1");
        assert_ne!(
            base,
            commit_v2_packet(&env, &packet),
            "dest_client must affect commitment"
        );

        packet.dest_client = String::from_str(&env, "07-tendermint-0");
        let mut p2 = payload.clone();
        p2.value = Bytes::from_slice(&env, b"world");
        packet.payloads = vec![&env, p2];
        assert_ne!(
            base,
            commit_v2_packet(&env, &packet),
            "payload value must affect commitment"
        );
    }

    #[test]
    fn commit_v2_acknowledgement_distinguishes_payload_count() {
        let env = test_env();
        let single =
            commit_v2_acknowledgement(&env, &vec![&env, Bytes::from_slice(&env, b"\x01\x02\x03")]);
        let two = commit_v2_acknowledgement(
            &env,
            &vec![
                &env,
                Bytes::from_slice(&env, b"\x01\x02\x03"),
                Bytes::from_slice(&env, b"\x04\x05\x06"),
            ],
        );
        assert_ne!(single, two);
    }
}
