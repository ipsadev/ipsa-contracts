mod common;

use common::*;
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, Env, String,
};
use stellar_ibc_router::{
    ics24::{commit_v2_acknowledgement, error_ack_hash},
    Packet, Payload,
};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_json, FungibleTokenPacketData, Token, TransferReceived,
};

#[contract]
pub struct Vault;

#[contractimpl]
impl Vault {
    pub fn on_ibc_transfer_received(env: Env, transfer_app: Address, received: TransferReceived) {
        transfer_app.require_auth();
        env.storage()
            .instance()
            .set(&symbol_short!("app"), &transfer_app);
        env.storage()
            .instance()
            .set(&symbol_short!("received"), &received);
    }

    pub fn received(env: Env) -> Option<TransferReceived> {
        env.storage().instance().get(&symbol_short!("received"))
    }

    pub fn transfer_app(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("app"))
    }
}

#[contract]
pub struct RejectingVault;

#[contractimpl]
impl RejectingVault {
    pub fn on_ibc_transfer_received(
        _env: Env,
        _transfer_app: Address,
        _received: TransferReceived,
    ) {
        panic!("the vault rejects the deposit")
    }
}

fn inbound(f: &Fixture, receiver: &Address, memo: &str) -> Packet {
    let packet_data = FungibleTokenPacketData {
        token: Token {
            denom: xlm(&f.env),
            amount: 500,
        },
        sender: String::from_str(&f.env, "cosmos1abc"),
        receiver: address_to_string(&f.env, receiver),
        memo: String::from_str(&f.env, memo),
    };

    Packet {
        sequence: 1,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.source_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            Payload {
                source_port: String::from_str(&f.env, "transfer"),
                dest_port: String::from_str(&f.env, "transfer"),
                version: String::from_str(&f.env, "ics20-1"),
                encoding: String::from_str(&f.env, ENCODING),
                value: encode_ics20_json(&f.env, &packet_data),
            },
        ],
    }
}

fn callback_memo(target: &Address) -> std::string::String {
    let address = target.to_string().to_string();

    format!("{{\"dest_callback\":{{\"address\":\"{address}\"}}}}")
}

fn receive(f: &Fixture, packet: &Packet) {
    f.env.ledger().set_timestamp(1_000);
    f.router
        .recv_packet(packet, &Bytes::from_slice(&f.env, b"proof"), &10);
}

fn success_ack(f: &Fixture) -> soroban_sdk::BytesN<32> {
    commit_v2_acknowledgement(
        &f.env,
        &vec![&f.env, Bytes::from_slice(&f.env, SUCCESS_ACK_JSON)],
    )
}

fn error_ack(f: &Fixture) -> soroban_sdk::BytesN<32> {
    let error: Bytes = error_ack_hash(&f.env).into();

    commit_v2_acknowledgement(&f.env, &vec![&f.env, error])
}

#[test]
fn a_transfer_with_a_destination_callback_credits_the_contract_and_calls_it() {
    let f = setup();
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    let voucher = register_voucher(&f, &denom);
    let vault = f.env.register(Vault, ());

    receive(&f, &inbound(&f, &vault, &callback_memo(&vault)));

    let received = VaultClient::new(&f.env, &vault)
        .received()
        .expect("the vault was called");

    assert_eq!(f.transfer.balance_of(&vault, &denom), 500);
    assert_eq!(received.amount, 500);
    assert_eq!(received.token, voucher);
    assert_eq!(received.sequence, 1);
    assert_eq!(received.sender, String::from_str(&f.env, "cosmos1abc"));
    assert_eq!(
        VaultClient::new(&f.env, &vault).transfer_app(),
        Some(f.transfer_addr.clone())
    );
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(success_ack(&f))
    );
}

#[test]
fn a_failing_callback_reverts_the_credit_and_acknowledges_an_error() {
    let f = setup();
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &denom);
    let vault = f.env.register(RejectingVault, ());

    receive(&f, &inbound(&f, &vault, &callback_memo(&vault)));

    assert_eq!(f.transfer.balance_of(&vault, &denom), 0);
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_ack(&f))
    );
}

#[test]
fn a_callback_to_a_contract_other_than_the_receiver_is_refused() {
    let f = setup();
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &denom);
    let receiver = Address::generate(&f.env);
    let vault = f.env.register(Vault, ());

    receive(&f, &inbound(&f, &receiver, &callback_memo(&vault)));

    assert_eq!(f.transfer.balance_of(&receiver, &denom), 0);
    assert!(VaultClient::new(&f.env, &vault).received().is_none());
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_ack(&f))
    );
}

#[test]
fn a_malformed_callback_is_refused() {
    let f = setup();
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &denom);
    let vault = f.env.register(Vault, ());

    receive(
        &f,
        &inbound(&f, &vault, "{\"dest_callback\":{\"address\":42}}"),
    );

    assert_eq!(f.transfer.balance_of(&vault, &denom), 0);
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_ack(&f))
    );
}

#[test]
fn a_transfer_without_a_callback_does_not_call_the_receiver() {
    let f = setup();
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &denom);
    let vault = f.env.register(RejectingVault, ());

    receive(&f, &inbound(&f, &vault, "{\"note\":\"plain transfer\"}"));

    assert_eq!(f.transfer.balance_of(&vault, &denom), 500);
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(success_ack(&f))
    );
}
