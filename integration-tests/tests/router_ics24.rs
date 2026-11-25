use soroban_sdk::{vec, Bytes, BytesN, Env, String};
use stellar_ibc_router::{
    ics24::{
        ack_commitment_path, commit_v2_acknowledgement, commit_v2_packet, error_ack_hash,
        packet_commitment_path, packet_receipt_path, ACK_COMMITMENT_DISCRIMINATOR,
        PACKET_COMMITMENT_DISCRIMINATOR, PACKET_RECEIPT_DISCRIMINATOR,
    },
    Packet, Payload,
};

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
