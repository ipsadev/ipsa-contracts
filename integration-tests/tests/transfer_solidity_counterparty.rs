use integration_tests::tendermint::{Entry, HEIGHT};
use integration_tests::transfer::{
    fund, register_local, setup, setup_committing, xlm, Fixture, COUNTERPARTY_CLIENT_ID,
    LOCAL_CLIENT_ID,
};
use soroban_sdk::{testutils::Address as _, vec, Address, Bytes, Env, String};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_abi, Error, FungibleTokenPacketData, OnTimeoutPacketCallback,
    Payload, Token, ENCODING_JSON, ENCODING_SOLIDITY_ABI,
};

const EVM_RECEIVER: &str = "0x06eCF411bE934d0eCb80a247508cA96c8030808E";

const UNIVERSAL_ERROR_ACK: [u8; 32] = [
    0x47, 0x74, 0xd4, 0xa5, 0x75, 0x99, 0x3f, 0x96, 0x3b, 0x1c, 0x06, 0x57, 0x37, 0x36, 0x61, 0x7a,
    0x45, 0x7a, 0xbe, 0xf8, 0x58, 0x91, 0x78, 0xdb, 0x8d, 0x10, 0xc9, 0x4b, 0x4a, 0xb5, 0x11, 0xab,
];

fn evm_packet(
    env: &Env,
    sequence: u64,
    sender: &Address,
    amount: i128,
) -> stellar_ibc_router::Packet {
    let value = encode_ics20_abi(
        env,
        &FungibleTokenPacketData {
            token: Token {
                denom: xlm(env),
                amount,
            },
            sender: address_to_string(env, sender),
            receiver: String::from_str(env, EVM_RECEIVER),
            memo: String::from_str(env, ""),
        },
    );

    stellar_ibc_router::Packet {
        sequence,
        source_client: String::from_str(env, LOCAL_CLIENT_ID),
        dest_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(env, "transfer"),
                dest_port: String::from_str(env, "transfer"),
                version: String::from_str(env, "ics20-1"),
                encoding: String::from_str(env, ENCODING_SOLIDITY_ABI),
                value,
            },
        ],
    }
}

fn send_to_evm(f: &Fixture, sender: &Address, amount: i128) -> (u64, stellar_ibc_router::Packet) {
    f.transfer.set_client_encoding(
        &f.source_client_id,
        &String::from_str(&f.env, ENCODING_SOLIDITY_ABI),
    );

    let sequence = f.transfer.initiate_transfer(
        sender,
        &f.source_client_id,
        &xlm(&f.env),
        &amount,
        &String::from_str(&f.env, EVM_RECEIVER),
        &2_000,
        &String::from_str(&f.env, ""),
    );

    (sequence, evm_packet(&f.env, sequence, sender, amount))
}

#[test]
fn a_client_defaults_to_json() {
    let f = setup();

    assert_eq!(
        f.transfer.client_encoding(&f.source_client_id),
        String::from_str(&f.env, ENCODING_JSON)
    );
}

#[test]
fn an_unsupported_encoding_cannot_be_configured() {
    let f = setup();

    let result = f.transfer.try_set_client_encoding(
        &f.source_client_id,
        &String::from_str(&f.env, "application/x-protobuf"),
    );

    assert_eq!(result, Err(Ok(Error::UnsupportedEncoding.into())));
}

#[test]
fn the_committed_packet_carries_the_solidity_abi_payload() {
    let env = Env::default();
    let sender = Address::generate(&env);
    let packet = evm_packet(&env, 1, &sender, 400);
    let committed_acks = vec![&env, Bytes::from_slice(&env, b"{\"result\":\"AQ==\"}")];
    let f = setup_committing(
        env,
        &[Entry::Acknowledgement(packet.clone(), committed_acks)],
    );
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    fund(&f, &stellar_asset_contract, &sender, 1_000);

    send_to_evm(&f, &sender, 400);

    f.router.acknowledge_packet(
        &packet,
        &vec![&f.env, Bytes::from_slice(&f.env, b"{\"result\":\"AQ==\"}")],
        &f.proofs[0].clone(),
        &HEIGHT,
    );

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 600);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 400);
}

#[test]
fn the_evm_universal_error_ack_refunds_from_the_abi_payload() {
    let env = Env::default();
    let sender = Address::generate(&env);
    let packet = evm_packet(&env, 1, &sender, 400);
    let committed_acks = vec![&env, Bytes::from_slice(&env, &UNIVERSAL_ERROR_ACK)];
    let f = setup_committing(
        env,
        &[Entry::Acknowledgement(packet.clone(), committed_acks)],
    );
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    fund(&f, &stellar_asset_contract, &sender, 1_000);

    send_to_evm(&f, &sender, 400);

    f.router.acknowledge_packet(
        &packet,
        &vec![&f.env, Bytes::from_slice(&f.env, &UNIVERSAL_ERROR_ACK)],
        &f.proofs[0].clone(),
        &HEIGHT,
    );

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 1_000);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 0);
}

#[test]
fn a_timeout_refunds_from_the_abi_payload() {
    let f = setup();
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &stellar_asset_contract, &sender, 1_000);

    let (_, packet) = send_to_evm(&f, &sender, 250);

    let payload = packet.payloads.get(0).unwrap();

    f.transfer.on_timeout_packet(&OnTimeoutPacketCallback {
        source_client: packet.source_client.clone(),
        dest_client: packet.dest_client.clone(),
        sequence: packet.sequence,
        payload: Payload {
            source_port: payload.source_port,
            dest_port: payload.dest_port,
            version: payload.version,
            encoding: payload.encoding,
            value: payload.value,
        },
    });

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 1_000);
}
