use soroban_sdk::{Bytes, Env, String};
use stellar_ibc_transfer::{
    decode_packet_abi, decode_payload, encode_ics20_abi, encode_payload, FungibleTokenPacketData,
    Token, ENCODING_JSON, ENCODING_SOLIDITY_ABI,
};

const SOLIDITY_VECTOR: &str = "000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000120000000000000000000000000000000000000000000000000000000000000018000000000000000000000000000000000000000000000003635c9adc5dea0000000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000000000000537472616e736665722f7374656c6c61722d746573746e65742d302f43444c5a46433353594a59445a54374b3637565a373548504a56494555564e49584634375a4732464232524d51515655324848474359534300000000000000000000000000000000000000000000000000000000000000000000000000000000000000003847414454594651594b4352574c4e47584856374f4e4255543356455442434d344d4452484f4f463758574b4a504a4c5a334c35584b49574f0000000000000000000000000000000000000000000000000000000000000000000000000000002a30783036654346343131624539333464306543623830613234373530386341393663383033303830384500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000568656c6c6f000000000000000000000000000000000000000000000000000000";

fn hex_bytes(env: &Env, hex: &str) -> Bytes {
    let raw: std::vec::Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect();

    Bytes::from_slice(env, &raw)
}

fn vector_packet(env: &Env) -> FungibleTokenPacketData {
    FungibleTokenPacketData {
        token: Token {
            denom: String::from_str(
                env,
                "transfer/stellar-testnet-0/CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
            ),
            amount: 1_000_000_000_000_000_000_000i128,
        },
        sender: String::from_str(env, "GADTYFQYKCRWLNGXHV7ONBUT3VETBCM4MDRHOOF7XWKJPJLZ3L5XKIWO"),
        receiver: String::from_str(env, "0x06eCF411bE934d0eCb80a247508cA96c8030808E"),
        memo: String::from_str(env, "hello"),
    }
}

#[test]
fn encoding_matches_solidity_abi_encode_byte_for_byte() {
    let env = Env::default();

    let encoded = encode_ics20_abi(&env, &vector_packet(&env));

    assert_eq!(encoded, hex_bytes(&env, SOLIDITY_VECTOR));
}

#[test]
fn decoding_the_solidity_vector_recovers_every_field() {
    let env = Env::default();
    let expected = vector_packet(&env);

    let decoded = decode_packet_abi(&env, &hex_bytes(&env, SOLIDITY_VECTOR));

    assert_eq!(decoded.token.denom, expected.token.denom);
    assert_eq!(decoded.token.amount, expected.token.amount);
    assert_eq!(decoded.sender, expected.sender);
    assert_eq!(decoded.receiver, expected.receiver);
    assert_eq!(decoded.memo, expected.memo);
}

#[test]
fn an_empty_memo_round_trips() {
    let env = Env::default();
    let mut packet = vector_packet(&env);
    packet.memo = String::from_str(&env, "");

    let decoded = decode_packet_abi(&env, &encode_ics20_abi(&env, &packet));

    assert_eq!(decoded.memo, packet.memo);
    assert_eq!(decoded.receiver, packet.receiver);
}

#[test]
fn payload_dispatch_follows_the_encoding_field() {
    let env = Env::default();
    let packet = vector_packet(&env);
    let abi = String::from_str(&env, ENCODING_SOLIDITY_ABI);
    let json = String::from_str(&env, ENCODING_JSON);

    let through_abi = decode_payload(&env, &abi, &encode_payload(&env, &abi, &packet));
    let through_json = decode_payload(&env, &json, &encode_payload(&env, &json, &packet));

    assert_eq!(through_abi.token.amount, packet.token.amount);
    assert_eq!(through_json.token.amount, packet.token.amount);
}

#[test]
#[should_panic]
fn an_unknown_encoding_is_refused() {
    let env = Env::default();

    decode_payload(
        &env,
        &String::from_str(&env, "application/x-protobuf"),
        &hex_bytes(&env, SOLIDITY_VECTOR),
    );
}

#[test]
#[should_panic]
fn a_json_payload_is_not_accepted_as_abi() {
    let env = Env::default();
    let json = Bytes::from_slice(
        &env,
        b"{\"denom\":\"uatom\",\"amount\":\"1\",\"sender\":\"a\",\"receiver\":\"b\",\"memo\":\"\"}",
    );

    decode_packet_abi(&env, &json);
}

#[test]
#[should_panic]
fn an_amount_beyond_i128_is_refused() {
    let env = Env::default();
    let mut raw = hex_bytes(&env, SOLIDITY_VECTOR);

    raw.set(32 + 3 * 32, 0x01);

    decode_packet_abi(&env, &raw);
}

#[test]
#[should_panic]
fn a_string_offset_past_the_end_is_refused() {
    let env = Env::default();
    let mut raw = hex_bytes(&env, SOLIDITY_VECTOR);

    raw.set(32 + 31, 0xff);

    decode_packet_abi(&env, &raw);
}
