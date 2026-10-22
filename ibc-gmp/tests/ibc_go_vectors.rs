use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, Address, Bytes, Env, String,
};
use stellar_ibc_gmp::{
    GmpAcknowledged, GmpPacketData, IbcGmpApp, IbcGmpAppClient, OnAcknowledgementPacketCallback,
    Payload,
};

const PROTOBUF: &str = "application/x-protobuf";

const JSON: &str = "application/json";

const PROTOBUF_PACKET: &str = "0a2d636f736d6f73316e7668766d39346a6e706d6c746c73706e656d776c676d7a66396c656d6d6a6d393234797739123843444c5a46433353594a59445a54374b3637565a373548504a56494555564e49584634375a4732464232524d5151565532484847435953431a03010203220463616c6c2a046d656d6f";

const JSON_PACKET: &str = "7b2273656e646572223a22636f736d6f73316e7668766d39346a6e706d6c746c73706e656d776c676d7a66396c656d6d6a6d393234797739222c227265636569766572223a2243444c5a46433353594a59445a54374b3637565a373548504a56494555564e49584634375a4732464232524d515156553248484743595343222c2273616c74223a2241514944222c227061796c6f6164223a225932467362413d3d222c226d656d6f223a226d656d6f227d";

const PROTOBUF_ACKNOWLEDGEMENT: &str = "0a096f6b2d726573756c74";

const JSON_ACKNOWLEDGEMENT: &str = "7b22726573756c74223a2262327374636d567a64577830227d";

const RESULT: &[u8] = b"ok-result";

#[contract]
pub struct Caller;

#[contractimpl]
impl Caller {
    pub fn on_gmp_acknowledgement(env: Env, gmp: Address, result: GmpAcknowledged) {
        gmp.require_auth();
        env.storage().instance().set(&symbol_short!("ack"), &result);
    }

    pub fn acknowledged(env: Env) -> Option<GmpAcknowledged> {
        env.storage().instance().get(&symbol_short!("ack"))
    }
}

fn hex(env: &Env, text: &str) -> Bytes {
    let bytes: std::vec::Vec<u8> = (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect();

    Bytes::from_slice(env, &bytes)
}

fn setup() -> (Env, IbcGmpAppClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let router = Address::generate(&env);
    let app_id = env.register(IbcGmpApp, (owner, router));

    (env.clone(), IbcGmpAppClient::new(&env, &app_id))
}

fn ibc_go_packet(env: &Env) -> GmpPacketData {
    GmpPacketData {
        sender: String::from_str(env, "cosmos1nvhvm94jnpmltlspnemwlgmzf9lemmjm924yw9"),
        receiver: String::from_str(
            env,
            "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        ),
        salt: Bytes::from_slice(env, &[1, 2, 3]),
        payload: Bytes::from_slice(env, b"call"),
        memo: String::from_str(env, "memo"),
    }
}

fn acknowledge(
    env: &Env,
    app: &IbcGmpAppClient,
    encoding: &str,
    acknowledgement: Bytes,
) -> GmpAcknowledged {
    let caller = env.register(Caller, ());
    let packet = GmpPacketData {
        sender: caller.to_string(),
        ..ibc_go_packet(env)
    };
    let encoding = String::from_str(env, encoding);

    app.on_acknowledgement_packet(&OnAcknowledgementPacketCallback {
        source_client: String::from_str(env, "07-tendermint-0"),
        dest_client: String::from_str(env, "08-wasm-5"),
        sequence: 1,
        payload: Payload {
            source_port: String::from_str(env, "gmpport"),
            dest_port: String::from_str(env, "gmpport"),
            version: String::from_str(env, "ics27-2"),
            value: app.encode_packet(&encoding, &packet),
            encoding,
        },
        acknowledgement,
    });

    CallerClient::new(env, &caller)
        .acknowledged()
        .expect("the caller was called")
}

fn assert_same(actual: &GmpPacketData, expected: &GmpPacketData) {
    assert_eq!(actual.sender, expected.sender);
    assert_eq!(actual.receiver, expected.receiver);
    assert_eq!(actual.salt, expected.salt);
    assert_eq!(actual.payload, expected.payload);
    assert_eq!(actual.memo, expected.memo);
}

#[test]
fn a_protobuf_packet_matches_ibc_go_byte_for_byte() {
    let (env, app) = setup();
    let encoding = String::from_str(&env, PROTOBUF);
    let expected = hex(&env, PROTOBUF_PACKET);

    assert_eq!(app.encode_packet(&encoding, &ibc_go_packet(&env)), expected);
    assert_same(
        &app.decode_packet(&encoding, &expected),
        &ibc_go_packet(&env),
    );
}

#[test]
fn a_json_packet_matches_ibc_go_byte_for_byte() {
    let (env, app) = setup();
    let encoding = String::from_str(&env, JSON);
    let expected = hex(&env, JSON_PACKET);

    assert_eq!(app.encode_packet(&encoding, &ibc_go_packet(&env)), expected);
    assert_same(
        &app.decode_packet(&encoding, &expected),
        &ibc_go_packet(&env),
    );
}

#[test]
fn an_ibc_go_protobuf_acknowledgement_reports_its_result() {
    let (env, app) = setup();
    let reported = acknowledge(&env, &app, PROTOBUF, hex(&env, PROTOBUF_ACKNOWLEDGEMENT));

    assert!(reported.success);
    assert_eq!(reported.result, Bytes::from_slice(&env, RESULT));
}

#[test]
fn an_ibc_go_json_acknowledgement_reports_its_result() {
    let (env, app) = setup();
    let reported = acknowledge(&env, &app, JSON, hex(&env, JSON_ACKNOWLEDGEMENT));

    assert!(reported.success);
    assert_eq!(reported.result, Bytes::from_slice(&env, RESULT));
}

#[test]
fn a_malformed_acknowledgement_is_reported_without_a_result() {
    let (env, app) = setup();
    let reported = acknowledge(&env, &app, PROTOBUF, Bytes::from_slice(&env, &[0xff, 0xff]));

    assert!(reported.success);
    assert!(reported.result.is_empty());
}

#[test]
fn a_protobuf_packet_with_an_unknown_field_is_refused() {
    let (env, app) = setup();
    let mut value = hex(&env, PROTOBUF_PACKET);

    value.append(&Bytes::from_slice(&env, &[0x32, 0x01, 0x00]));

    assert!(app
        .try_decode_packet(&String::from_str(&env, PROTOBUF), &value)
        .is_err());
}
