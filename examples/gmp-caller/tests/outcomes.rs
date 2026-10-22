use ipsa_gmp_caller::{Error, GmpAcknowledged, IpsaGmpCaller, IpsaGmpCallerClient, Outcome};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, Env, String,
};
use stellar_ibc_gmp::{
    GmpPacketData, IbcGmpApp, IbcGmpAppClient, OnTimeoutPacketCallback, Payload as GmpPayload,
};
use stellar_ibc_router::{IbcRouter, IbcRouterClient, Packet, Payload};
use stellar_mock_light_client::MockLightClient;

const SOLIDITY_ABI: &str = "application/x-solidity-abi";

const RECEIVER: &str = "0x0000000000000000000000000000000000000001";

struct Fixture {
    env: Env,
    router: IbcRouterClient<'static>,
    gmp: IbcGmpAppClient<'static>,
    caller: IpsaGmpCallerClient<'static>,
    client_id: String,
    counterparty_id: String,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000);

    let owner = Address::generate(&env);
    let router_id = env.register(IbcRouter, (owner.clone(),));
    let router = IbcRouterClient::new(&env, &router_id);
    let light_client = env.register(MockLightClient, ());
    router.register_client_type(&String::from_str(&env, "mock"), &light_client);

    let client_id = router.create_client(
        &String::from_str(&env, "mock"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );
    let counterparty_id = String::from_str(&env, "stellar-testnet-2");
    router.register_counterparty(
        &client_id,
        &counterparty_id,
        &vec![&env, Bytes::from_slice(&env, b"ibc")],
    );

    let gmp_id = env.register(IbcGmpApp, (owner.clone(), router_id.clone()));
    router.register_port(&String::from_str(&env, "gmpport"), &gmp_id);

    let caller_id = env.register(IpsaGmpCaller, (owner, gmp_id.clone()));

    Fixture {
        router,
        gmp: IbcGmpAppClient::new(&env, &gmp_id),
        caller: IpsaGmpCallerClient::new(&env, &caller_id),
        client_id,
        counterparty_id,
        env,
    }
}

fn send(f: &Fixture) -> (u64, Packet) {
    let encoding = String::from_str(&f.env, SOLIDITY_ABI);
    let receiver = String::from_str(&f.env, RECEIVER);
    let salt = Bytes::new(&f.env);
    let payload = Bytes::from_slice(&f.env, &[0xde, 0xad, 0xbe, 0xef]);
    let memo = String::from_str(&f.env, "");

    let sequence = f.caller.send_call(
        &f.client_id,
        &receiver,
        &salt,
        &payload,
        &2_000,
        &memo,
        &encoding,
    );

    let packet_data = GmpPacketData {
        sender: f.caller.address.to_string(),
        receiver,
        salt,
        payload,
        memo,
    };

    let packet = Packet {
        sequence,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            Payload {
                source_port: String::from_str(&f.env, "gmpport"),
                dest_port: String::from_str(&f.env, "gmpport"),
                version: String::from_str(&f.env, "ics27-2"),
                encoding: encoding.clone(),
                value: f.gmp.encode_packet(&encoding, &packet_data),
            },
        ],
    };

    (sequence, packet)
}

fn acknowledge(f: &Fixture, packet: &Packet, acknowledgement: Bytes) {
    f.router.acknowledge_packet(
        packet,
        &vec![&f.env, acknowledgement],
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );
}

fn abi_word(env: &Env, value: u32) -> Bytes {
    let mut word = [0u8; 32];
    word[28..].copy_from_slice(&value.to_be_bytes());

    Bytes::from_array(env, &word)
}

fn abi_acknowledgement(env: &Env, result: &Bytes) -> Bytes {
    let mut out = abi_word(env, 32);
    let padding = (32 - result.len() % 32) % 32;

    out.append(&abi_word(env, 32));
    out.append(&abi_word(env, result.len()));
    out.append(result);
    out.append(&Bytes::from_slice(env, &[0u8; 32][..padding as usize]));

    out
}

fn error_acknowledgement(env: &Env) -> Bytes {
    let hash = env
        .crypto()
        .sha256(&Bytes::from_slice(env, b"UNIVERSAL_ERROR_ACKNOWLEDGEMENT"))
        .to_array();

    Bytes::from_array(env, &hash)
}

#[test]
fn a_sent_call_is_pending_until_its_acknowledgement() {
    let f = setup();

    let (sequence, _) = send(&f);

    assert_eq!(sequence, 1);
    assert_eq!(
        f.caller.outcome(&f.client_id, &sequence),
        Some(Outcome::Pending)
    );
}

#[test]
fn a_successful_call_is_recorded_with_its_result() {
    let f = setup();
    let (sequence, packet) = send(&f);
    let result = Bytes::from_slice(&f.env, b"result");
    let acknowledgement = abi_acknowledgement(&f.env, &result);

    acknowledge(&f, &packet, acknowledgement.clone());

    let Some(Outcome::Acknowledged(acknowledged)) = f.caller.outcome(&f.client_id, &sequence)
    else {
        panic!("the call was not acknowledged");
    };

    assert!(acknowledged.success);
    assert_eq!(acknowledged.result, result);
    assert_eq!(acknowledged.acknowledgement, acknowledgement);
}

#[test]
fn a_failed_call_is_recorded_as_a_failure() {
    let f = setup();
    let (sequence, packet) = send(&f);

    acknowledge(&f, &packet, error_acknowledgement(&f.env));

    let Some(Outcome::Acknowledged(acknowledged)) = f.caller.outcome(&f.client_id, &sequence)
    else {
        panic!("the call was not acknowledged");
    };

    assert!(!acknowledged.success);
}

#[test]
fn a_timed_out_call_is_recorded_as_timed_out() {
    let f = setup();
    let (sequence, packet) = send(&f);
    let payload = packet.payloads.get(0).unwrap();

    f.gmp.on_timeout_packet(&OnTimeoutPacketCallback {
        source_client: packet.source_client.clone(),
        dest_client: packet.dest_client.clone(),
        sequence,
        payload: GmpPayload {
            source_port: payload.source_port,
            dest_port: payload.dest_port,
            version: payload.version,
            encoding: payload.encoding,
            value: payload.value,
        },
    });

    assert!(matches!(
        f.caller.outcome(&f.client_id, &sequence),
        Some(Outcome::TimedOut(_))
    ));
}

#[test]
fn a_callback_from_another_contract_is_refused() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let result = GmpAcknowledged {
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_id.clone(),
        sequence: 1,
        success: true,
        result: Bytes::new(&f.env),
        acknowledgement: Bytes::new(&f.env),
    };

    let refused = f.caller.try_on_gmp_acknowledgement(&impostor, &result);

    assert_eq!(refused, Err(Ok(Error::UntrustedGmpApp.into())));
}
