use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, xdr::ToXdr, Address, Bytes, Env,
    String,
};
use stellar_ibc_gmp::{
    GmpAcknowledged, GmpPacketData, GmpTimedOut, IbcGmpApp, IbcGmpAppClient,
    OnAcknowledgementPacketCallback, OnTimeoutPacketCallback, Payload,
};

const ENCODING: &str = "application/x-soroban-xdr";

#[contract]
pub struct Caller;

#[contractimpl]
impl Caller {
    pub fn on_gmp_acknowledgement(env: Env, gmp: Address, result: GmpAcknowledged) {
        gmp.require_auth();
        env.storage().instance().set(&symbol_short!("gmp"), &gmp);
        env.storage().instance().set(&symbol_short!("ack"), &result);
    }

    pub fn on_gmp_timeout(env: Env, gmp: Address, result: GmpTimedOut) {
        gmp.require_auth();
        env.storage().instance().set(&symbol_short!("gmp"), &gmp);
        env.storage()
            .instance()
            .set(&symbol_short!("timeout"), &result);
    }

    pub fn acknowledged(env: Env) -> Option<GmpAcknowledged> {
        env.storage().instance().get(&symbol_short!("ack"))
    }

    pub fn timed_out(env: Env) -> Option<GmpTimedOut> {
        env.storage().instance().get(&symbol_short!("timeout"))
    }

    pub fn gmp(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("gmp"))
    }
}

#[contract]
pub struct BrokenCaller;

#[contractimpl]
impl BrokenCaller {
    pub fn on_gmp_acknowledgement(_env: Env, _gmp: Address, _result: GmpAcknowledged) {
        panic!("the caller's callback fails")
    }

    pub fn on_gmp_timeout(_env: Env, _gmp: Address, _result: GmpTimedOut) {
        panic!("the caller's callback fails")
    }
}

fn setup() -> (Env, Address, IbcGmpAppClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let router = Address::generate(&env);
    let app_id = env.register(IbcGmpApp, (owner, router));
    let app = IbcGmpAppClient::new(&env, &app_id);

    (env, app_id, app)
}

fn payload(env: &Env, sender: &Address) -> Payload {
    let packet = GmpPacketData {
        sender: sender.to_string(),
        receiver: String::from_str(env, "0x0000000000000000000000000000000000000001"),
        salt: Bytes::new(env),
        payload: Bytes::from_slice(env, b"call"),
        memo: String::from_str(env, ""),
    };

    Payload {
        source_port: String::from_str(env, "gmpport"),
        dest_port: String::from_str(env, "gmpport"),
        version: String::from_str(env, "ics27-2"),
        encoding: String::from_str(env, ENCODING),
        value: packet.to_xdr(env),
    }
}

fn acknowledgement(
    env: &Env,
    sender: &Address,
    acknowledgement: Bytes,
) -> OnAcknowledgementPacketCallback {
    OnAcknowledgementPacketCallback {
        source_client: String::from_str(env, "sepolia-0"),
        dest_client: String::from_str(env, "stellar-testnet-2"),
        sequence: 7,
        payload: payload(env, sender),
        acknowledgement,
    }
}

fn timeout(env: &Env, sender: &Address) -> OnTimeoutPacketCallback {
    OnTimeoutPacketCallback {
        source_client: String::from_str(env, "sepolia-0"),
        dest_client: String::from_str(env, "stellar-testnet-2"),
        sequence: 7,
        payload: payload(env, sender),
    }
}

fn error_acknowledgement(env: &Env) -> Bytes {
    let hash = env
        .crypto()
        .sha256(&Bytes::from_slice(env, b"UNIVERSAL_ERROR_ACKNOWLEDGEMENT"))
        .to_array();

    Bytes::from_array(env, &hash)
}

#[test]
fn a_successful_call_reports_its_result_to_the_calling_contract() {
    let (env, app_id, app) = setup();
    let caller = env.register(Caller, ());
    let result = Bytes::from_slice(&env, b"result");

    app.on_acknowledgement_packet(&acknowledgement(&env, &caller, result.clone()));

    let reported = CallerClient::new(&env, &caller)
        .acknowledged()
        .expect("the caller was called");

    assert!(reported.success);
    assert_eq!(reported.acknowledgement, result);
    assert_eq!(reported.sequence, 7);
    assert_eq!(reported.source_client, String::from_str(&env, "sepolia-0"));
    assert_eq!(CallerClient::new(&env, &caller).gmp(), Some(app_id));
}

#[test]
fn a_failed_call_reports_failure_to_the_calling_contract() {
    let (env, _, app) = setup();
    let caller = env.register(Caller, ());

    app.on_acknowledgement_packet(&acknowledgement(&env, &caller, error_acknowledgement(&env)));

    let reported = CallerClient::new(&env, &caller)
        .acknowledged()
        .expect("the caller was called");

    assert!(!reported.success);
}

#[test]
fn a_timed_out_call_reports_the_timeout_to_the_calling_contract() {
    let (env, _, app) = setup();
    let caller = env.register(Caller, ());

    app.on_timeout_packet(&timeout(&env, &caller));

    let reported = CallerClient::new(&env, &caller)
        .timed_out()
        .expect("the caller was called");

    assert_eq!(reported.sequence, 7);
}

#[test]
fn a_failing_callback_does_not_block_the_acknowledgement_or_timeout() {
    let (env, _, app) = setup();
    let caller = env.register(BrokenCaller, ());

    app.on_acknowledgement_packet(&acknowledgement(
        &env,
        &caller,
        Bytes::from_slice(&env, b"ok"),
    ));
    app.on_timeout_packet(&timeout(&env, &caller));
}

#[test]
fn an_account_sender_is_not_called_back() {
    let (env, _, app) = setup();
    let account = Address::from_str(
        &env,
        "GAPCJ2GXGIY2VRLK6W4DDY443R3JDMJ3K5LQBMUJL4LDSX34MXQMSHVD",
    );

    app.on_acknowledgement_packet(&acknowledgement(
        &env,
        &account,
        Bytes::from_slice(&env, b"ok"),
    ));
    app.on_timeout_packet(&timeout(&env, &account));
}
