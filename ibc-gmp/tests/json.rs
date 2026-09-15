#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, Env, String};
use stellar_ibc_gmp::{GmpPacketData, IbcGmpApp, IbcGmpAppClient};

fn app(env: &Env) -> IbcGmpAppClient<'_> {
    let owner = Address::generate(env);
    let router = Address::generate(env);

    IbcGmpAppClient::new(env, &env.register(IbcGmpApp, (owner, router)))
}

fn packet(env: &Env, salt: &[u8], payload: &[u8], memo: &str) -> GmpPacketData {
    GmpPacketData {
        sender: String::from_str(env, "cosmos1abc"),
        receiver: String::from_str(env, "CAAA"),
        salt: Bytes::from_slice(env, salt),
        payload: Bytes::from_slice(env, payload),
        memo: String::from_str(env, memo),
    }
}

fn round_trip(env: &Env, app: &IbcGmpAppClient, original: &GmpPacketData) -> GmpPacketData {
    let json = String::from_str(env, "application/json");
    let encoded = app.encode_packet(&json, original);

    app.decode_packet(&json, &encoded)
}

#[test]
fn a_packet_survives_a_json_round_trip() {
    let env = Env::default();
    let app = app(&env);

    let original = packet(&env, &[1, 2, 3], &[9, 8, 7, 6], "hello");
    let decoded = round_trip(&env, &app, &original);

    assert_eq!(decoded.sender, original.sender);
    assert_eq!(decoded.receiver, original.receiver);
    assert_eq!(decoded.salt, original.salt);
    assert_eq!(decoded.payload, original.payload);
    assert_eq!(decoded.memo, original.memo);
}

// base64 pads differently at each length modulo 3, so all three must round trip.
#[test]
fn every_base64_padding_case_round_trips() {
    let env = Env::default();
    let app = app(&env);

    for payload in [&[1u8][..], &[1, 2][..], &[1, 2, 3][..], &[1, 2, 3, 4][..]] {
        let original = packet(&env, &[0xff], payload, "");
        let decoded = round_trip(&env, &app, &original);

        assert_eq!(
            decoded.payload,
            original.payload,
            "length {}",
            payload.len()
        );
    }
}

#[test]
fn empty_byte_fields_round_trip() {
    let env = Env::default();
    let app = app(&env);

    let original = packet(&env, &[], &[], "");
    let decoded = round_trip(&env, &app, &original);

    assert_eq!(decoded.salt.len(), 0);
    assert_eq!(decoded.payload.len(), 0);
}

#[test]
fn a_packet_written_by_another_chain_decodes() {
    let env = Env::default();
    let app = app(&env);

    let wire = Bytes::from_slice(
        &env,
        br#"{"sender":"cosmos1abc","receiver":"CAAA","salt":"AQID","payload":"CQgHBg==","memo":"hi"}"#,
    );

    let decoded = app.decode_packet(&String::from_str(&env, "application/json"), &wire);

    assert_eq!(decoded.sender, String::from_str(&env, "cosmos1abc"));
    assert_eq!(decoded.salt, Bytes::from_slice(&env, &[1, 2, 3]));
    assert_eq!(decoded.payload, Bytes::from_slice(&env, &[9, 8, 7, 6]));
    assert_eq!(decoded.memo, String::from_str(&env, "hi"));
}

#[test]
#[should_panic]
fn an_unknown_encoding_is_refused() {
    let env = Env::default();
    let app = app(&env);

    app.decode_packet(
        &String::from_str(&env, "application/x-solidity-abi"),
        &Bytes::from_slice(&env, b"{}"),
    );
}
