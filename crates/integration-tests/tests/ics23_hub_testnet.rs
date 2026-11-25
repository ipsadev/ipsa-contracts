use ipsa_ics23::{verify_membership, verify_non_membership};
use serde_json::Value;
use soroban_sdk::{Bytes, Env, Vec};

const FIXTURE: &str = include_str!("../fixtures/hub-testnet-08-wasm-464.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn hex(value: &Value) -> std::vec::Vec<u8> {
    let text = value.as_str().unwrap();

    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn root(f: &Value) -> [u8; 32] {
    hex(&f["root"]).try_into().unwrap()
}

fn merkle_path(env: &Env, prefix: &[&[u8]], key: &[u8]) -> Vec<Bytes> {
    let mut path = Vec::new(env);

    for (index, segment) in prefix.iter().enumerate() {
        let mut bytes = Bytes::from_slice(env, segment);

        if index == prefix.len() - 1 {
            bytes.append(&Bytes::from_slice(env, key));
        }

        path.push_back(bytes);
    }

    path
}

fn joined(path: &Vec<Bytes>) -> std::vec::Vec<u8> {
    let mut out = std::vec::Vec::new();

    for segment in path.iter() {
        let mut bytes = std::vec![0u8; segment.len() as usize];
        segment.copy_into_slice(&mut bytes);
        out.extend_from_slice(&bytes);
    }

    out
}

#[test]
fn a_hub_acknowledgement_is_proven_under_the_migrated_prefix() {
    let env = Env::default();
    let f = fixture();
    let ack = &f["acknowledgement"];
    let key = hex(&ack["key"]);

    let migrated = merkle_path(&env, &[b"ibc", b""], &key);
    let legacy = merkle_path(&env, &[b"ibc"], &key);

    assert_eq!(joined(&migrated), joined(&legacy));
    assert!(verify_membership(
        &env,
        &root(&f),
        &Bytes::from_slice(&env, &hex(&ack["proof"])),
        &migrated,
        &Bytes::from_slice(&env, &hex(&ack["value"]))
    ));
}

#[test]
fn a_hub_acknowledgement_is_refused_under_the_legacy_single_segment() {
    let env = Env::default();
    let f = fixture();
    let ack = &f["acknowledgement"];

    assert!(!verify_membership(
        &env,
        &root(&f),
        &Bytes::from_slice(&env, &hex(&ack["proof"])),
        &merkle_path(&env, &[b"ibc"], &hex(&ack["key"])),
        &Bytes::from_slice(&env, &hex(&ack["value"]))
    ));
}

#[test]
fn a_hub_acknowledgement_is_refused_when_the_store_key_is_split() {
    let env = Env::default();
    let f = fixture();
    let ack = &f["acknowledgement"];

    assert!(!verify_membership(
        &env,
        &root(&f),
        &Bytes::from_slice(&env, &hex(&ack["proof"])),
        &merkle_path(&env, &[b"ib", b"c"], &hex(&ack["key"])),
        &Bytes::from_slice(&env, &hex(&ack["value"]))
    ));
}

#[test]
fn a_hub_acknowledgement_is_refused_with_another_value() {
    let env = Env::default();
    let f = fixture();
    let ack = &f["acknowledgement"];

    let mut value = hex(&ack["value"]);
    value[0] ^= 0xff;

    assert!(!verify_membership(
        &env,
        &root(&f),
        &Bytes::from_slice(&env, &hex(&ack["proof"])),
        &merkle_path(&env, &[b"ibc", b""], &hex(&ack["key"])),
        &Bytes::from_slice(&env, &value)
    ));
}

#[test]
fn a_hub_receipt_is_proven_absent_under_the_migrated_prefix() {
    let env = Env::default();
    let f = fixture();
    let absent = &f["absent_receipt"];

    assert!(verify_non_membership(
        &env,
        &root(&f),
        &Bytes::from_slice(&env, &hex(&absent["proof"])),
        &merkle_path(&env, &[b"ibc", b""], &hex(&absent["key"]))
    ));
}

#[test]
fn a_hub_receipt_absence_is_refused_when_the_store_key_is_split() {
    let env = Env::default();
    let f = fixture();
    let absent = &f["absent_receipt"];

    assert!(!verify_non_membership(
        &env,
        &root(&f),
        &Bytes::from_slice(&env, &hex(&absent["proof"])),
        &merkle_path(&env, &[b"ib", b"c"], &hex(&absent["key"]))
    ));
}
