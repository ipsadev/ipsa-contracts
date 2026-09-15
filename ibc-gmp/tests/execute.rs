#![cfg(test)]

mod account_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/contract/ipsa_gmp_account.wasm");
}

use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Address, Bytes, Env, IntoVal, String, Symbol, Val,
    Vec,
};
use stellar_ibc_gmp::{
    GmpCall, GmpPacketData, IbcGmpApp, IbcGmpAppClient, OnRecvPacketCallback, Payload,
};

#[contract]
pub struct Target;

#[contractimpl]
impl Target {
    pub fn remember(env: Env, caller: Address, value: u32) -> u32 {
        caller.require_auth();
        env.storage()
            .instance()
            .set(&symbol_short!("caller"), &caller);
        env.storage()
            .instance()
            .set(&symbol_short!("value"), &value);
        value
    }

    pub fn caller(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("caller"))
    }

    pub fn value(env: Env) -> Option<u32> {
        env.storage().instance().get(&symbol_short!("value"))
    }
}

struct Fixture<'a> {
    env: Env,
    app: IbcGmpAppClient<'a>,
    target: TargetClient<'a>,
    router: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let router = Address::generate(&env);

    let app_id = env.register(IbcGmpApp, (owner, router.clone()));
    let app = IbcGmpAppClient::new(&env, &app_id);

    let wasm = env.deployer().upload_contract_wasm(account_contract::WASM);
    app.set_account_wasm(&wasm);

    let target_id = env.register(Target, ());

    Fixture {
        target: TargetClient::new(&env, &target_id),
        env,
        app,
        router,
    }
}

fn recv(f: &Fixture, sender: &str, salt: &[u8], call: &GmpCall) -> OnRecvPacketCallback {
    let packet = GmpPacketData {
        sender: String::from_str(&f.env, sender),
        receiver: f.target.address.to_string(),
        salt: Bytes::from_slice(&f.env, salt),
        payload: call.clone().to_xdr(&f.env),
        memo: String::from_str(&f.env, ""),
    };

    OnRecvPacketCallback {
        source_client: String::from_str(&f.env, "08-wasm-0"),
        dest_client: String::from_str(&f.env, "07-tendermint-0"),
        sequence: 1,
        payload: Payload {
            source_port: String::from_str(&f.env, "gmpport"),
            dest_port: String::from_str(&f.env, "gmpport"),
            version: String::from_str(&f.env, "ics27-2"),
            encoding: String::from_str(&f.env, "application/x-soroban-xdr"),
            value: packet.to_xdr(&f.env),
        },
    }
}

fn call_for(f: &Fixture, account: &Address, value: u32) -> GmpCall {
    let args: Vec<Val> = vec![&f.env, account.into_val(&f.env), value.into_val(&f.env)];

    GmpCall {
        target: f.target.address.clone(),
        function: Symbol::new(&f.env, "remember"),
        args,
    }
}

#[test]
fn a_packet_creates_the_account_and_executes_the_call_as_it() {
    let f = setup();

    let client = String::from_str(&f.env, "07-tendermint-0");
    let sender = String::from_str(&f.env, "cosmos1abc");
    let salt = Bytes::from_slice(&f.env, &[7u8]);

    let derived = f.app.account_address(&client, &sender, &salt);
    assert!(
        !derived.materialized,
        "nothing is deployed before the packet"
    );

    let call = call_for(&f, &derived.address, 42);
    let ack = f.app.on_recv_packet(&recv(&f, "cosmos1abc", &[7u8], &call));

    assert!(!ack.is_empty(), "the packet must be acknowledged");

    assert_eq!(
        f.target.caller(),
        Some(derived.address.clone()),
        "the target must see the derived account as the caller, not the gmp app"
    );
    assert_eq!(f.target.value(), Some(42));

    let after = f.app.account_address(&client, &sender, &salt);
    assert!(after.materialized, "the account is materialized once used");
    assert_eq!(
        after.address, derived.address,
        "the address was predictable"
    );
}

#[test]
fn a_second_packet_reuses_the_same_account() {
    let f = setup();

    let client = String::from_str(&f.env, "07-tendermint-0");
    let sender = String::from_str(&f.env, "cosmos1abc");
    let salt = Bytes::from_slice(&f.env, &[7u8]);

    let derived = f.app.account_address(&client, &sender, &salt);

    f.app.on_recv_packet(&recv(
        &f,
        "cosmos1abc",
        &[7u8],
        &call_for(&f, &derived.address, 1),
    ));
    f.app.on_recv_packet(&recv(
        &f,
        "cosmos1abc",
        &[7u8],
        &call_for(&f, &derived.address, 2),
    ));

    assert_eq!(f.target.value(), Some(2));
    assert_eq!(f.target.caller(), Some(derived.address));
}

#[test]
#[should_panic]
fn a_packet_for_another_port_is_refused() {
    let f = setup();

    let derived = f.app.account_address(
        &String::from_str(&f.env, "07-tendermint-0"),
        &String::from_str(&f.env, "cosmos1abc"),
        &Bytes::from_slice(&f.env, &[7u8]),
    );

    let mut callback = recv(&f, "cosmos1abc", &[7u8], &call_for(&f, &derived.address, 1));
    callback.payload.dest_port = String::from_str(&f.env, "transfer");

    f.app.on_recv_packet(&callback);
}

#[test]
#[should_panic]
fn a_packet_declaring_another_encoding_is_refused() {
    let f = setup();

    let derived = f.app.account_address(
        &String::from_str(&f.env, "07-tendermint-0"),
        &String::from_str(&f.env, "cosmos1abc"),
        &Bytes::from_slice(&f.env, &[7u8]),
    );

    let mut callback = recv(&f, "cosmos1abc", &[7u8], &call_for(&f, &derived.address, 1));
    callback.payload.encoding = String::from_str(&f.env, "application/json");

    f.app.on_recv_packet(&callback);
}

#[test]
#[should_panic]
fn a_call_targeting_the_router_is_refused() {
    let f = setup();

    let derived = f.app.account_address(
        &String::from_str(&f.env, "07-tendermint-0"),
        &String::from_str(&f.env, "cosmos1abc"),
        &Bytes::from_slice(&f.env, &[7u8]),
    );

    let mut call = call_for(&f, &derived.address, 1);
    call.target = f.router.clone();

    f.app.on_recv_packet(&recv(&f, "cosmos1abc", &[7u8], &call));
}
