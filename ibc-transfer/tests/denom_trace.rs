mod common;

mod voucher_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32v1-none/contract/stellar_ibc_voucher_token.wasm"
    );
}

use common::{fund, register_local, setup, xlm, Fixture};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, String};
use stellar_ibc_transfer::{
    address_to_string, decode_packet_abi, encode_ics20_abi, received_denom,
    FungibleTokenPacketData, OnRecvPacketCallback, Payload, Token, ENCODING_SOLIDITY_ABI,
};

const EVM_TOKEN: &str = "0x39a000607761b47fbb7e80d51d6abd7af969f70b";

fn text(f: &Fixture, value: &str) -> String {
    String::from_str(&f.env, value)
}

fn client(id: &String) -> std::string::String {
    let mut buf = [0u8; 128];
    let len = id.len() as usize;
    id.copy_into_slice(&mut buf[..len]);

    core::str::from_utf8(&buf[..len]).unwrap().to_owned()
}

fn from_evm(f: &Fixture, receiver: &Address, denom: &String, amount: i128) -> OnRecvPacketCallback {
    OnRecvPacketCallback {
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.source_client_id.clone(),
        sequence: 1,
        payload: Payload {
            source_port: text(f, "transfer"),
            dest_port: text(f, "transfer"),
            version: text(f, "ics20-1"),
            encoding: text(f, ENCODING_SOLIDITY_ABI),
            value: encode_ics20_abi(
                &f.env,
                &FungibleTokenPacketData {
                    token: Token {
                        denom: denom.clone(),
                        amount,
                    },
                    sender: text(f, "0x06ecf411be934d0ecb80a247508ca96c8030808e"),
                    receiver: address_to_string(&f.env, receiver),
                    memo: text(f, ""),
                },
            ),
        },
    }
}

fn with_voucher_wasm(f: &Fixture) {
    let hash = f
        .env
        .deployer()
        .upload_contract_wasm(voucher_contract::WASM);

    f.transfer.set_voucher_wasm(&hash);
}

#[test]
fn a_foreign_denom_gains_this_sides_port_and_client() {
    let f = setup();

    let denom = received_denom(
        &f.env,
        &text(&f, "transfer"),
        &f.counterparty_client_id,
        &text(&f, "transfer"),
        &f.source_client_id,
        &text(&f, EVM_TOKEN),
    );

    let expected = format!("transfer/{}/{EVM_TOKEN}", client(&f.source_client_id));

    assert_eq!(denom, text(&f, &expected));
}

#[test]
fn a_denom_carrying_the_senders_prefix_is_coming_home() {
    let f = setup();
    let sent = format!("transfer/{}/XLM", client(&f.counterparty_client_id));

    let denom = received_denom(
        &f.env,
        &text(&f, "transfer"),
        &f.counterparty_client_id,
        &text(&f, "transfer"),
        &f.source_client_id,
        &text(&f, &sent),
    );

    assert_eq!(denom, xlm(&f.env));
}

#[test]
fn another_clients_prefix_is_foreign_not_returning() {
    let f = setup();

    let denom = received_denom(
        &f.env,
        &text(&f, "transfer"),
        &f.counterparty_client_id,
        &text(&f, "transfer"),
        &f.source_client_id,
        &text(&f, "transfer/some-other-0/XLM"),
    );

    let expected = format!(
        "transfer/{}/transfer/some-other-0/XLM",
        client(&f.source_client_id)
    );

    assert_eq!(denom, text(&f, &expected));
}

#[test]
#[should_panic]
fn a_bare_prefix_with_no_base_denom_is_refused() {
    let f = setup();
    let sent = format!("transfer/{}/", client(&f.counterparty_client_id));

    received_denom(
        &f.env,
        &text(&f, "transfer"),
        &f.counterparty_client_id,
        &text(&f, "transfer"),
        &f.source_client_id,
        &text(&f, &sent),
    );
}

#[test]
fn an_evm_token_arrives_as_a_traced_voucher_and_goes_back_with_its_trace() {
    let f = setup();
    with_voucher_wasm(&f);
    let holder = Address::generate(&f.env);
    let voucher = text(
        &f,
        &format!("transfer/{}/{EVM_TOKEN}", client(&f.source_client_id)),
    );

    f.transfer
        .on_recv_packet(&from_evm(&f, &holder, &text(&f, EVM_TOKEN), 1_000));

    assert_eq!(f.transfer.balance_of(&holder, &voucher), 1_000);

    f.transfer
        .set_client_encoding(&f.source_client_id, &text(&f, ENCODING_SOLIDITY_ABI));

    let sequence = f.transfer.initiate_transfer(
        &holder,
        &f.source_client_id,
        &voucher,
        &400,
        &text(&f, "0x06ecf411be934d0ecb80a247508ca96c8030808e"),
        &2_000,
        &text(&f, ""),
    );

    assert_eq!(f.transfer.balance_of(&holder, &voucher), 600);

    let packet = FungibleTokenPacketData {
        token: Token {
            denom: voucher.clone(),
            amount: 400,
        },
        sender: address_to_string(&f.env, &holder),
        receiver: text(&f, "0x06ecf411be934d0ecb80a247508ca96c8030808e"),
        memo: text(&f, ""),
    };

    assert!(f
        .router
        .packet_commitment(&f.source_client_id, &sequence)
        .is_some());
    assert_eq!(
        decode_packet_abi(&f.env, &encode_ics20_abi(&f.env, &packet))
            .token
            .denom,
        voucher
    );
}

#[test]
fn a_returning_stellar_asset_is_released_from_escrow() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let receiver = Address::generate(&f.env);
    fund(&f, &sac, &f.transfer_addr, 1_000);

    let sent = format!("transfer/{}/XLM", client(&f.counterparty_client_id));

    f.transfer
        .on_recv_packet(&from_evm(&f, &receiver, &text(&f, &sent), 250));

    assert_eq!(f.transfer.balance_of(&receiver, &xlm(&f.env)), 250);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 750);
}
