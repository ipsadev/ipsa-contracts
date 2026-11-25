use integration_tests::tendermint::{Entry, HEIGHT};
use integration_tests::transfer::*;
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

fn inbound(env: &Env, receiver: &Address, memo: &str) -> Packet {
    let packet_data = FungibleTokenPacketData {
        token: Token {
            denom: xlm(env),
            amount: 500,
        },
        sender: String::from_str(env, "cosmos1abc"),
        receiver: address_to_string(env, receiver),
        memo: String::from_str(env, memo),
    };

    Packet {
        sequence: 1,
        source_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            Payload {
                source_port: String::from_str(env, "transfer"),
                dest_port: String::from_str(env, "transfer"),
                version: String::from_str(env, "ics20-1"),
                encoding: String::from_str(env, ENCODING),
                value: encode_ics20_json(env, &packet_data),
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
    f.router.recv_packet(packet, &f.proofs[0].clone(), &HEIGHT);
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
    let env = soroban_sdk::Env::default();
    let vault = env.register(Vault, ());
    let packet = inbound(&env, &vault, &callback_memo(&vault));
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    let voucher = register_voucher(&f, &denom);

    receive(&f, &packet);

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
    let env = soroban_sdk::Env::default();
    let vault = env.register(RejectingVault, ());
    let packet = inbound(&env, &vault, &callback_memo(&vault));
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &denom);

    receive(&f, &packet);

    assert_eq!(f.transfer.balance_of(&vault, &denom), 0);
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_ack(&f))
    );
}

#[test]
fn a_callback_to_a_contract_other_than_the_receiver_is_refused() {
    let env = soroban_sdk::Env::default();
    let vault = env.register(Vault, ());
    let receiver = Address::generate(&env);
    let packet = inbound(&env, &receiver, &callback_memo(&vault));
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &denom);

    receive(&f, &packet);

    assert_eq!(f.transfer.balance_of(&receiver, &denom), 0);
    assert!(VaultClient::new(&f.env, &vault).received().is_none());
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_ack(&f))
    );
}

#[test]
fn a_malformed_callback_is_refused() {
    let env = soroban_sdk::Env::default();
    let vault = env.register(Vault, ());
    let packet = inbound(&env, &vault, "{\"dest_callback\":{\"address\":42}}");
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &denom);

    receive(&f, &packet);

    assert_eq!(f.transfer.balance_of(&vault, &denom), 0);
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_ack(&f))
    );
}

#[test]
fn a_transfer_without_a_callback_does_not_call_the_receiver() {
    let env = soroban_sdk::Env::default();
    let vault = env.register(RejectingVault, ());
    let packet = inbound(&env, &vault, "{\"note\":\"plain transfer\"}");
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &denom);

    receive(&f, &packet);

    assert_eq!(f.transfer.balance_of(&vault, &denom), 500);
    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(success_ack(&f))
    );
}
