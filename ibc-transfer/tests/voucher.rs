mod common;

mod voucher_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32v1-none/contract/stellar_ibc_voucher_token.wasm"
    );
}

use common::{build_inbound_packet, setup, SUCCESS_ACK_BYTE};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, String};
use stellar_ibc_transfer::{OnRecvPacketCallback, Payload};

fn recv_callback(
    f: &common::Fixture,
    receiver: &Address,
    denom: &String,
    amount: i128,
) -> OnRecvPacketCallback {
    OnRecvPacketCallback {
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.source_client_id.clone(),
        sequence: 1,
        payload: Payload {
            source_port: String::from_str(&f.env, "transfer"),
            dest_port: String::from_str(&f.env, "transfer"),
            version: String::from_str(&f.env, "ics20-1"),
            encoding: String::from_str(&f.env, "application/json"),
            value: build_inbound_packet(&f.env, "cosmos1abc", receiver, denom, amount),
        },
    }
}

fn voucher_denom(f: &common::Fixture, denom: &str) -> String {
    let mut buf = [0u8; 128];
    let len = f.source_client_id.len() as usize;
    f.source_client_id.copy_into_slice(&mut buf[..len]);
    let client = core::str::from_utf8(&buf[..len]).unwrap();

    String::from_str(&f.env, &format!("transfer/{client}/{denom}"))
}

#[test]
fn recv_unregistered_denom_auto_deploys_voucher_and_mints() {
    let f = setup();
    let wasm_hash = f
        .env
        .deployer()
        .upload_contract_wasm(voucher_contract::WASM);
    f.transfer.set_voucher_wasm(&wasm_hash);

    let receiver = Address::generate(&f.env);
    let stake = String::from_str(&f.env, "stake");

    let ack = f
        .transfer
        .on_recv_packet(&recv_callback(&f, &receiver, &stake, 1_000));

    assert_eq!(ack, Bytes::from_slice(&f.env, &[SUCCESS_ACK_BYTE]));
    assert_eq!(
        f.transfer
            .balance_of(&receiver, &voucher_denom(&f, "stake")),
        1_000
    );
}

#[test]
fn send_foreign_voucher_burns_it() {
    let f = setup();
    let wasm_hash = f
        .env
        .deployer()
        .upload_contract_wasm(voucher_contract::WASM);
    f.transfer.set_voucher_wasm(&wasm_hash);

    let holder = Address::generate(&f.env);
    let stake = String::from_str(&f.env, "stake");
    f.transfer
        .on_recv_packet(&recv_callback(&f, &holder, &stake, 1_000));

    f.transfer.initiate_transfer(
        &holder,
        &f.source_client_id,
        &voucher_denom(&f, "stake"),
        &400,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );

    assert_eq!(
        f.transfer.balance_of(&holder, &voucher_denom(&f, "stake")),
        600
    );
}

#[test]
fn recv_reuses_the_same_voucher_across_calls() {
    let f = setup();
    let wasm_hash = f
        .env
        .deployer()
        .upload_contract_wasm(voucher_contract::WASM);
    f.transfer.set_voucher_wasm(&wasm_hash);

    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let stake = String::from_str(&f.env, "stake");

    f.transfer
        .on_recv_packet(&recv_callback(&f, &a, &stake, 300));
    f.transfer
        .on_recv_packet(&recv_callback(&f, &b, &stake, 700));

    assert_eq!(f.transfer.balance_of(&a, &voucher_denom(&f, "stake")), 300);
    assert_eq!(f.transfer.balance_of(&b, &voucher_denom(&f, "stake")), 700);
}
