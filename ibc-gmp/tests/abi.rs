#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};
use stellar_ibc_gmp::{GmpPacketData, IbcGmpApp, IbcGmpAppClient};

const SOLIDITY_ABI: &str = "application/x-solidity-abi";

const PACKET_FROM_CAST: &str = "000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000001e0000000000000000000000000000000000000000000000000000000000000002a30783941336535643643326230453746663161346631433961316537623141356331443245334634413500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003843444c5a46433353594a59445a54374b3637565a373548504a56494555564e49584634375a4732464232524d5151565532484847435953430000000000000000000000000000000000000000000000000000000000000000000000000000000201020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004deadbeef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000568656c6c6f000000000000000000000000000000000000000000000000000000";

fn app(env: &Env) -> IbcGmpAppClient<'_> {
    let owner = Address::generate(env);
    let router = Address::generate(env);

    IbcGmpAppClient::new(env, &env.register(IbcGmpApp, (owner, router)))
}

fn hex(env: &Env, text: &str) -> Bytes {
    let bytes: std::vec::Vec<u8> = (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex"))
        .collect();

    Bytes::from_slice(env, &bytes)
}

fn packet(env: &Env) -> GmpPacketData {
    GmpPacketData {
        sender: String::from_str(env, "0x9A3e5d6C2b0E7Ff1a4f1C9a1e7b1A5c1D2E3F4A5"),
        receiver: String::from_str(
            env,
            "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        ),
        salt: Bytes::from_slice(env, &[0x01, 0x02]),
        payload: Bytes::from_slice(env, &[0xde, 0xad, 0xbe, 0xef]),
        memo: String::from_str(env, "hello"),
    }
}

fn same(left: &GmpPacketData, right: &GmpPacketData) -> bool {
    left.sender == right.sender
        && left.receiver == right.receiver
        && left.salt == right.salt
        && left.payload == right.payload
        && left.memo == right.memo
}

#[test]
fn a_packet_encodes_to_the_bytes_solidity_produces() {
    let env = Env::default();
    let encoding = String::from_str(&env, SOLIDITY_ABI);

    let encoded = app(&env).encode_packet(&encoding, &packet(&env));

    assert_eq!(encoded, hex(&env, PACKET_FROM_CAST));
}

#[test]
fn a_packet_from_solidity_decodes() {
    let env = Env::default();
    let encoding = String::from_str(&env, SOLIDITY_ABI);

    let decoded = app(&env).decode_packet(&encoding, &hex(&env, PACKET_FROM_CAST));

    assert!(same(&decoded, &packet(&env)));
}

#[test]
fn empty_fields_and_a_payload_on_a_word_boundary_round_trip() {
    let env = Env::default();
    let encoding = String::from_str(&env, SOLIDITY_ABI);
    let app = app(&env);
    let original = GmpPacketData {
        sender: String::from_str(&env, ""),
        receiver: String::from_str(&env, "0x0000000000000000000000000000000000000001"),
        salt: Bytes::new(&env),
        payload: Bytes::from_slice(&env, &[7u8; 64]),
        memo: String::from_str(&env, ""),
    };

    let decoded = app.decode_packet(&encoding, &app.encode_packet(&encoding, &original));

    assert!(same(&decoded, &original));
}

#[test]
fn a_truncated_packet_is_rejected() {
    let env = Env::default();
    let encoding = String::from_str(&env, SOLIDITY_ABI);
    let full = hex(&env, PACKET_FROM_CAST);

    let result = app(&env).try_decode_packet(&encoding, &full.slice(..full.len() - 64));

    assert!(result.is_err());
}

#[test]
fn an_offset_with_high_bits_set_is_rejected() {
    let env = Env::default();
    let encoding = String::from_str(&env, SOLIDITY_ABI);
    let mut corrupted = hex(&env, PACKET_FROM_CAST);
    corrupted.set(32, 0xff);

    let result = app(&env).try_decode_packet(&encoding, &corrupted);

    assert!(result.is_err());
}
