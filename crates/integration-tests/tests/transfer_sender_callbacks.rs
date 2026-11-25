use integration_tests::tendermint::{Entry, HEIGHT};
use integration_tests::transfer::{
    fund, outbound_packet_in, outbound_timeout, register_local, setup_committing, xlm, Fixture,
    SUCCESS_ACK_JSON,
};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, contracttype, symbol_short,
    testutils::Address as _,
    token, vec, Address, Bytes, Env, IntoVal, String, Symbol,
};
use stellar_ibc_router::Packet;
use stellar_ibc_transfer::{IbcTransferAppClient, TransferAcknowledged, TransferTimedOut};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Acknowledged(bool, i128),
    TimedOut(i128),
}

#[contract]
pub struct SendingContract;

#[contractimpl]
impl SendingContract {
    pub fn send(env: Env, transfer: Address, client_id: String, amount: i128) -> u64 {
        authorize_escrow(&env, &transfer, amount);

        IbcTransferAppClient::new(&env, &transfer).initiate_transfer(
            &env.current_contract_address(),
            &client_id,
            &String::from_str(&env, "XLM"),
            &amount,
            &String::from_str(&env, "cosmos1abc"),
            &2_000,
            &String::from_str(&env, ""),
        )
    }

    pub fn set_token(env: Env, token: Address) {
        env.storage()
            .instance()
            .set(&symbol_short!("token"), &token);
    }

    pub fn on_transfer_acknowledgement(env: Env, transfer: Address, result: TransferAcknowledged) {
        transfer.require_auth();

        let outcome = Outcome::Acknowledged(result.success, balance(&env));

        env.storage()
            .instance()
            .set(&symbol_short!("outcome"), &outcome);
    }

    pub fn on_transfer_timeout(env: Env, transfer: Address, _result: TransferTimedOut) {
        transfer.require_auth();

        let outcome = Outcome::TimedOut(balance(&env));

        env.storage()
            .instance()
            .set(&symbol_short!("outcome"), &outcome);
    }

    pub fn outcome(env: Env) -> Option<Outcome> {
        env.storage().instance().get(&symbol_short!("outcome"))
    }
}

fn authorize_escrow(env: &Env, transfer: &Address, amount: i128) {
    let token: Address = env
        .storage()
        .instance()
        .get(&symbol_short!("token"))
        .unwrap();
    let escrow = SubContractInvocation {
        context: ContractContext {
            contract: token,
            fn_name: Symbol::new(env, "transfer"),
            args: (env.current_contract_address(), transfer.clone(), amount).into_val(env),
        },
        sub_invocations: vec![env],
    };

    env.authorize_as_current_contract(vec![env, InvokerContractAuthEntry::Contract(escrow)]);
}

fn balance(env: &Env) -> i128 {
    let token: Address = env
        .storage()
        .instance()
        .get(&symbol_short!("token"))
        .unwrap();

    token::Client::new(env, &token).balance(&env.current_contract_address())
}

#[contract]
pub struct FailingSender;

#[contractimpl]
impl FailingSender {
    pub fn set_token(env: Env, token: Address) {
        env.storage()
            .instance()
            .set(&symbol_short!("token"), &token);
    }

    pub fn send(env: Env, transfer: Address, client_id: String, amount: i128) -> u64 {
        SendingContract::send(env, transfer, client_id, amount)
    }

    pub fn on_transfer_acknowledgement(
        _env: Env,
        _transfer: Address,
        _result: TransferAcknowledged,
    ) {
        panic!("the sender rejects the acknowledgement")
    }

    pub fn on_transfer_timeout(_env: Env, _transfer: Address, _result: TransferTimedOut) {
        panic!("the sender rejects the timeout")
    }
}

fn wire_sending_contract(
    f: &Fixture,
    contract: &Address,
) -> (SendingContractClient<'static>, Address) {
    let token = register_local(f, &xlm(&f.env));
    let client = SendingContractClient::new(&f.env, contract);

    client.set_token(&token);
    fund(f, &token, contract, 1_000);

    (client, token)
}

fn acknowledge(f: &Fixture, packet: &Packet, acknowledgement: &[u8]) {
    f.router.acknowledge_packet(
        packet,
        &vec![&f.env, Bytes::from_slice(&f.env, acknowledgement)],
        &f.proofs[0].clone(),
        &HEIGHT,
    );
}

#[test]
fn a_contract_sender_learns_its_transfer_succeeded() {
    let env = soroban_sdk::Env::default();
    let contract = env.register(SendingContract, ());
    let packet = outbound_packet_in(&env, 1, &contract, 400);
    let committed = vec![&env, Bytes::from_slice(&env, SUCCESS_ACK_JSON)];
    let f = setup_committing(env, &[Entry::Acknowledgement(packet.clone(), committed)]);
    let (client, _token) = wire_sending_contract(&f, &contract);

    client.send(&f.transfer_addr, &f.source_client_id, &400);
    acknowledge(&f, &packet, SUCCESS_ACK_JSON);

    assert_eq!(client.outcome(), Some(Outcome::Acknowledged(true, 600)));
}

#[test]
fn a_contract_sender_is_refunded_before_it_hears_of_an_error_acknowledgement() {
    let env = soroban_sdk::Env::default();
    let contract = env.register(SendingContract, ());
    let packet = outbound_packet_in(&env, 1, &contract, 400);
    let committed = vec![&env, Bytes::from_slice(&env, b"\xff\xff")];
    let f = setup_committing(env, &[Entry::Acknowledgement(packet.clone(), committed)]);
    let (client, _token) = wire_sending_contract(&f, &contract);

    client.send(&f.transfer_addr, &f.source_client_id, &400);
    acknowledge(&f, &packet, b"\xff\xff");

    assert_eq!(client.outcome(), Some(Outcome::Acknowledged(false, 1_000)));
}

#[test]
fn a_contract_sender_is_refunded_before_it_hears_of_a_timeout() {
    let env = soroban_sdk::Env::default();
    let contract = env.register(SendingContract, ());
    let f = setup_committing(env, &[]);
    let (client, _token) = wire_sending_contract(&f, &contract);
    let sequence = client.send(&f.transfer_addr, &f.source_client_id, &400);

    f.transfer
        .on_timeout_packet(&outbound_timeout(&f, sequence, &contract, 400));

    assert_eq!(client.outcome(), Some(Outcome::TimedOut(1_000)));
}

#[test]
fn a_failing_sender_callback_does_not_block_the_refund() {
    let env = soroban_sdk::Env::default();
    let contract = env.register(FailingSender, ());
    let packet = outbound_packet_in(&env, 1, &contract, 400);
    let committed = vec![&env, Bytes::from_slice(&env, b"\xff\xff")];
    let f = setup_committing(env, &[Entry::Acknowledgement(packet.clone(), committed)]);
    let token = register_local(&f, &xlm(&f.env));
    let client = FailingSenderClient::new(&f.env, &contract);

    client.set_token(&token);
    fund(&f, &token, &contract, 1_000);

    client.send(&f.transfer_addr, &f.source_client_id, &400);
    let second = client.send(&f.transfer_addr, &f.source_client_id, &400);

    acknowledge(&f, &packet, b"\xff\xff");
    f.transfer
        .on_timeout_packet(&outbound_timeout(&f, second, &contract, 400));

    assert_eq!(token::Client::new(&f.env, &token).balance(&contract), 1_000);
}

#[test]
fn an_account_sender_is_refunded_without_a_callback() {
    let env = soroban_sdk::Env::default();
    let sender = Address::generate(&env);
    let packet = outbound_packet_in(&env, 1, &sender, 400);
    let committed = vec![&env, Bytes::from_slice(&env, b"\xff\xff")];
    let f = setup_committing(env, &[Entry::Acknowledgement(packet.clone(), committed)]);
    let token = register_local(&f, &xlm(&f.env));

    fund(&f, &token, &sender, 1_000);

    f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &400,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );

    acknowledge(&f, &packet, b"\xff\xff");

    assert_eq!(token::Client::new(&f.env, &token).balance(&sender), 1_000);
}
