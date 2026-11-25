use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};
use stellar_ibc_transfer::{
    address_to_string, decode_packet, encode_ics20_json, FungibleTokenPacketData, Token,
};

#[test]
fn ics20_json_codec_round_trip() {
    let env = Env::default();
    let receiver = Address::generate(&env);

    let original = FungibleTokenPacketData {
        token: Token {
            denom: String::from_str(&env, "transfer/channel-0/uatom"),
            amount: 123_456_789i128,
        },
        sender: String::from_str(&env, "cosmos1abcdefghijklmnop"),
        receiver: address_to_string(&env, &receiver),
        memo: String::from_str(&env, "hello"),
    };

    let encoded = encode_ics20_json(&env, &original);
    let decoded = decode_packet(&env, &encoded);

    assert_eq!(decoded.token.denom, original.token.denom);
    assert_eq!(decoded.token.amount, original.token.amount);
    assert_eq!(decoded.sender, original.sender);
    assert_eq!(decoded.receiver, original.receiver);
    assert_eq!(decoded.memo, original.memo);
}

#[test]
fn decode_packet_parses_real_ibc_go_json() {
    let env = Env::default();
    let blob = b"{\"denom\":\"uatom\",\"amount\":\"1000000\",\"sender\":\"cosmos1sender\",\"receiver\":\"GABC123\",\"memo\":\"\"}";
    let value = Bytes::from_slice(&env, blob);

    let decoded = decode_packet(&env, &value);

    assert_eq!(decoded.token.denom, String::from_str(&env, "uatom"));
    assert_eq!(decoded.token.amount, 1_000_000i128);
    assert_eq!(decoded.sender, String::from_str(&env, "cosmos1sender"));
    assert_eq!(decoded.receiver, String::from_str(&env, "GABC123"));
    assert_eq!(decoded.memo, String::from_str(&env, ""));
}

#[test]
fn decode_packet_tolerates_whitespace_reordering_and_missing_memo() {
    let env = Env::default();
    let blob = b"{ \"amount\": \"42\" , \"receiver\":\"GDEST\", \"sender\" : \"cosmos1x\", \"denom\":\"uosmo\" }";
    let value = Bytes::from_slice(&env, blob);

    let decoded = decode_packet(&env, &value);

    assert_eq!(decoded.token.denom, String::from_str(&env, "uosmo"));
    assert_eq!(decoded.token.amount, 42i128);
    assert_eq!(decoded.sender, String::from_str(&env, "cosmos1x"));
    assert_eq!(decoded.receiver, String::from_str(&env, "GDEST"));
    assert_eq!(decoded.memo, String::from_str(&env, ""));
}

#[test]
fn decode_packet_decodes_json_escapes_in_memo() {
    let env = Env::default();
    let blob = b"{\"denom\":\"uatom\",\"amount\":\"1\",\"sender\":\"cosmos1x\",\"receiver\":\"GDEST\",\"memo\":\"a\\\"b\\\\c\\u0041\"}";
    let value = Bytes::from_slice(&env, blob);

    let decoded = decode_packet(&env, &value);

    assert_eq!(decoded.memo, String::from_str(&env, "a\"b\\cA"));
}

#[test]
#[should_panic]
fn decode_packet_rejects_non_numeric_amount() {
    let env = Env::default();
    let blob = b"{\"denom\":\"uatom\",\"amount\":\"12x\",\"sender\":\"s\",\"receiver\":\"r\",\"memo\":\"\"}";
    let value = Bytes::from_slice(&env, blob);

    decode_packet(&env, &value);
}

fn packet_with_memo(env: &Env, memo: &str) -> FungibleTokenPacketData {
    FungibleTokenPacketData {
        token: Token {
            denom: String::from_str(env, "XLM"),
            amount: 1000,
        },
        sender: String::from_str(env, "GAPCJ2GX"),
        receiver: String::from_str(env, "cosmos1victim"),
        memo: String::from_str(env, memo),
    }
}

fn as_utf8(bytes: &Bytes) -> std::string::String {
    let mut out = std::vec::Vec::new();
    for b in bytes.iter() {
        out.push(b);
    }
    std::string::String::from_utf8(out).expect("encoded json is utf-8")
}

#[test]
fn an_injected_field_cannot_escape_its_json_string() {
    let env = Env::default();
    let packet = packet_with_memo(&env, "x\",\"receiver\":\"cosmos1attacker");

    let json = as_utf8(&encode_ics20_json(&env, &packet));

    assert_eq!(
        json.matches("\"receiver\":").count(),
        1,
        "a memo must not be able to introduce a second receiver key: {json}"
    );
    assert!(json.contains("cosmos1victim"));
    assert!(
        json.contains("\\\",\\\"receiver\\\""),
        "the injected quotes must be escaped: {json}"
    );
}

#[test]
fn a_quote_and_backslash_survive_the_round_trip() {
    let env = Env::default();
    let packet = packet_with_memo(&env, "a\"b\\c");

    let decoded = decode_packet(&env, &encode_ics20_json(&env, &packet));

    assert_eq!(decoded.memo, String::from_str(&env, "a\"b\\c"));
    assert_eq!(decoded.receiver, String::from_str(&env, "cosmos1victim"));
}

#[test]
fn control_bytes_are_encoded_as_escapes() {
    let env = Env::default();
    let packet = packet_with_memo(&env, "a\nb\tc");

    let json = as_utf8(&encode_ics20_json(&env, &packet));

    assert!(json.contains("a\\nb\\tc"), "got: {json}");
    assert!(!json.contains('\n'), "a raw newline breaks the json string");
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn an_oversized_field_is_rejected_rather_than_overflowing() {
    let env = Env::default();
    let long = "a".repeat(513);
    let packet = packet_with_memo(&env, &long);

    encode_ics20_json(&env, &packet);
}
