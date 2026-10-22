mod account_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/contract/ipsa_gmp_account.wasm");
}

use ipsa_test_light_client::TestLightClient;
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    vec,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};
use stellar_ibc_gmp::{GmpCall, GmpPacketData, IbcGmpApp, IbcGmpAppClient};
use stellar_ibc_router::{
    ics24::{commit_v2_acknowledgement, error_ack_hash},
    IbcRouter, IbcRouterClient, Packet, Payload,
};

const PROTOBUF: &str = "application/x-protobuf";

const REMOTE_SENDER: &str = "cosmos1nvhvm94jnpmltlspnemwlgmzf9lemmjm924yw9";

const COUNTERPARTY_CLIENT: &str = "08-wasm-5";

#[contract]
pub struct Treasury;

#[contractimpl]
impl Treasury {
    pub fn __constructor(env: Env, remote_account: Address) {
        env.storage()
            .instance()
            .set(&symbol_short!("remote"), &remote_account);
    }

    pub fn set_limit(env: Env, limit: u32) -> u32 {
        let remote_account: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("remote"))
            .unwrap();

        remote_account.require_auth();

        env.storage()
            .instance()
            .set(&symbol_short!("limit"), &limit);

        limit
    }

    pub fn limit(env: Env) -> Option<u32> {
        env.storage().instance().get(&symbol_short!("limit"))
    }
}

struct Fixture {
    env: Env,
    router: IbcRouterClient<'static>,
    gmp: IbcGmpAppClient<'static>,
    client_id: String,
}

fn setup() -> Fixture {
    let env = Env::default();

    env.mock_all_auths();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000);

    let owner = Address::generate(&env);
    let router_id = env.register(IbcRouter, (owner.clone(),));
    let router = IbcRouterClient::new(&env, &router_id);
    let light_client = env.register(TestLightClient, ());

    router.register_client_type(&String::from_str(&env, "test"), &light_client);

    let client_id = router.create_client(
        &String::from_str(&env, "test"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );

    router.register_counterparty(
        &client_id,
        &String::from_str(&env, COUNTERPARTY_CLIENT),
        &vec![&env, Bytes::from_slice(&env, b"ibc")],
    );

    let gmp_id = env.register(IbcGmpApp, (owner, router_id));
    let gmp = IbcGmpAppClient::new(&env, &gmp_id);
    let account_wasm = env.deployer().upload_contract_wasm(account_contract::WASM);

    gmp.set_account_wasm(&account_wasm);
    router.register_port(&String::from_str(&env, "gmpport"), &gmp_id);

    Fixture {
        env,
        router,
        gmp,
        client_id,
    }
}

fn deploy_treasury(f: &Fixture, sender: &str) -> TreasuryClient<'static> {
    let remote_account = f
        .gmp
        .account_address(
            &f.client_id,
            &String::from_str(&f.env, sender),
            &Bytes::new(&f.env),
        )
        .address;
    let id = f.env.register(Treasury, (remote_account,));

    TreasuryClient::new(&f.env, &id)
}

fn packet(f: &Fixture, sender: &str, receiver: &String, target: &Address) -> Packet {
    let args: Vec<Val> = vec![&f.env, 250u32.into_val(&f.env)];
    let call = GmpCall {
        target: target.clone(),
        function: Symbol::new(&f.env, "set_limit"),
        args,
    };
    let data = GmpPacketData {
        sender: String::from_str(&f.env, sender),
        receiver: receiver.clone(),
        salt: Bytes::new(&f.env),
        payload: call.to_xdr(&f.env),
        memo: String::from_str(&f.env, ""),
    };
    let encoding = String::from_str(&f.env, PROTOBUF);

    Packet {
        sequence: 1,
        source_client: String::from_str(&f.env, COUNTERPARTY_CLIENT),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            Payload {
                source_port: String::from_str(&f.env, "gmpport"),
                dest_port: String::from_str(&f.env, "gmpport"),
                version: String::from_str(&f.env, "ics27-2"),
                value: f.gmp.encode_packet(&encoding, &data),
                encoding,
            },
        ],
    }
}

fn receive(f: &Fixture, packet: &Packet) -> Option<BytesN<32>> {
    f.router
        .recv_packet(packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    f.router.acknowledgement(&f.client_id, &1)
}

fn error_acknowledgement(env: &Env) -> BytesN<32> {
    let error: Bytes = error_ack_hash(env).into();

    commit_v2_acknowledgement(env, &vec![env, error])
}

fn protobuf_acknowledgement(env: &Env, result: Val) -> Bytes {
    let result = result.to_xdr(env);
    let mut out = Bytes::from_slice(env, &[0x0a]);
    let mut length = result.len();

    while length >= 0x80 {
        out.push_back((length as u8 & 0x7f) | 0x80);
        length >>= 7;
    }

    out.push_back(length as u8);
    out.append(&result);

    out
}

#[test]
fn a_call_from_the_trusted_remote_sender_runs_through_the_router() {
    let f = setup();
    let treasury = deploy_treasury(&f, REMOTE_SENDER);
    let receiver = treasury.address.to_string();

    f.env.set_auths(&[]);

    let acknowledgement = receive(&f, &packet(&f, REMOTE_SENDER, &receiver, &treasury.address));
    let expected = protobuf_acknowledgement(&f.env, 250u32.into_val(&f.env));

    assert_eq!(treasury.limit(), Some(250));
    assert_eq!(
        acknowledgement,
        Some(commit_v2_acknowledgement(&f.env, &vec![&f.env, expected]))
    );
}

#[test]
fn a_call_from_another_remote_sender_is_refused() {
    let f = setup();
    let treasury = deploy_treasury(&f, REMOTE_SENDER);
    let receiver = treasury.address.to_string();

    f.env.set_auths(&[]);

    let acknowledgement = receive(
        &f,
        &packet(&f, "cosmos1stranger", &receiver, &treasury.address),
    );

    assert_eq!(treasury.limit(), None);
    assert_eq!(acknowledgement, Some(error_acknowledgement(&f.env)));
}

#[test]
fn a_receiver_that_is_not_the_call_target_is_refused() {
    let f = setup();
    let treasury = deploy_treasury(&f, REMOTE_SENDER);
    let other = deploy_treasury(&f, REMOTE_SENDER);

    f.env.set_auths(&[]);

    let acknowledgement = receive(
        &f,
        &packet(
            &f,
            REMOTE_SENDER,
            &other.address.to_string(),
            &treasury.address,
        ),
    );

    assert_eq!(treasury.limit(), None);
    assert_eq!(acknowledgement, Some(error_acknowledgement(&f.env)));
}

#[test]
fn an_empty_receiver_lets_the_call_name_its_target() {
    let f = setup();
    let treasury = deploy_treasury(&f, REMOTE_SENDER);

    f.env.set_auths(&[]);

    receive(
        &f,
        &packet(
            &f,
            REMOTE_SENDER,
            &String::from_str(&f.env, ""),
            &treasury.address,
        ),
    );

    assert_eq!(treasury.limit(), Some(250));
}
