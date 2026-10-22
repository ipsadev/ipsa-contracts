use ipsa_test_light_client::TestLightClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Bytes, Env, String,
};
use stellar_ibc_router::{IbcRouter, IbcRouterClient, Packet, Payload};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_json, FungibleTokenPacketData, IbcTransferApp,
    IbcTransferAppClient, Token,
};

const SUCCESS_ACK_BYTE: u8 = 0x01;
const SOURCE_VERSION: &str = "ics20-1";
const SOURCE_ENCODING: &str = "application/json";

struct Net {
    env: Env,
    router: IbcRouterClient<'static>,
    transfer: IbcTransferAppClient<'static>,
    transfer_addr: Address,
    client_id: String,
    counterparty_id: String,
}

fn xlm(env: &Env) -> String {
    String::from_str(env, "XLM")
}

fn register_local(net: &Net) -> Address {
    let issuer = Address::generate(&net.env);
    let sac = net.env.register_stellar_asset_contract_v2(issuer).address();
    net.transfer.register_asset(&xlm(&net.env), &sac, &false);

    sac
}

fn voucher_denom(net: &Net) -> String {
    let mut traced = std::string::String::from("transfer/");

    traced.push_str(&net.client_id.to_string());
    traced.push_str("/XLM");

    String::from_str(&net.env, &traced)
}

fn register_voucher(net: &Net) {
    let sac = net
        .env
        .register_stellar_asset_contract_v2(net.transfer_addr.clone())
        .address();

    net.transfer
        .register_asset(&voucher_denom(net), &sac, &true);
}

fn fund(net: &Net, sac: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(&net.env, sac).mint(to, &amount);
}

fn setup() -> Net {
    let env = Env::default();
    env.mock_all_auths();

    let router_admin = Address::generate(&env);
    let router_addr = env.register(IbcRouter, (router_admin,));
    let router = IbcRouterClient::new(&env, &router_addr);

    let lc_id = env.register(TestLightClient, ());
    router.register_client_type(&String::from_str(&env, "test"), &lc_id);

    let client_id = router.create_client(
        &String::from_str(&env, "test"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );
    let counterparty_id = String::from_str(&env, "07-tendermint-0");
    router.register_counterparty(
        &client_id,
        &counterparty_id,
        &vec![&env, Bytes::from_slice(&env, b"ibc")],
    );

    let transfer_admin = Address::generate(&env);
    let transfer_addr = env.register(IbcTransferApp, (router_addr.clone(), transfer_admin));
    let transfer = IbcTransferAppClient::new(&env, &transfer_addr);
    router.register_port(&String::from_str(&env, "transfer"), &transfer_addr);

    Net {
        env,
        router,
        transfer,
        transfer_addr,
        client_id,
        counterparty_id,
    }
}

fn inbound_packet(net: &Net, receiver: &Address, amount: i128, sequence: u64) -> Packet {
    let pkt = FungibleTokenPacketData {
        token: Token {
            denom: xlm(&net.env),
            amount,
        },
        sender: String::from_str(&net.env, "cosmos1abc"),
        receiver: address_to_string(&net.env, receiver),
        memo: String::from_str(&net.env, ""),
    };

    Packet {
        sequence,
        source_client: net.counterparty_id.clone(),
        dest_client: net.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &net.env,
            Payload {
                source_port: String::from_str(&net.env, "transfer"),
                dest_port: String::from_str(&net.env, "transfer"),
                version: String::from_str(&net.env, SOURCE_VERSION),
                encoding: String::from_str(&net.env, SOURCE_ENCODING),
                value: encode_ics20_json(&net.env, &pkt),
            },
        ],
    }
}

fn outbound_packet(net: &Net, sender: &Address, amount: i128, sequence: u64) -> Packet {
    let pkt = FungibleTokenPacketData {
        token: Token {
            denom: xlm(&net.env),
            amount,
        },
        sender: address_to_string(&net.env, sender),
        receiver: String::from_str(&net.env, "cosmos1abc"),
        memo: String::from_str(&net.env, ""),
    };

    Packet {
        sequence,
        source_client: net.client_id.clone(),
        dest_client: net.counterparty_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &net.env,
            Payload {
                source_port: String::from_str(&net.env, "transfer"),
                dest_port: String::from_str(&net.env, "transfer"),
                version: String::from_str(&net.env, SOURCE_VERSION),
                encoding: String::from_str(&net.env, SOURCE_ENCODING),
                value: encode_ics20_json(&net.env, &pkt),
            },
        ],
    }
}

#[test]
fn stellar_to_cosmos_round_trip_with_success_ack() {
    let net = setup();
    let sac = register_local(&net);
    let sender = Address::generate(&net.env);
    fund(&net, &sac, &sender, 1_000);

    let seq = net.transfer.initiate_transfer(
        &sender,
        &net.client_id,
        &xlm(&net.env),
        &400,
        &String::from_str(&net.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&net.env, ""),
    );
    assert_eq!(net.transfer.balance_of(&sender, &xlm(&net.env)), 600);
    assert_eq!(
        net.transfer.balance_of(&net.transfer_addr, &xlm(&net.env)),
        400
    );
    assert!(net.router.packet_commitment(&net.client_id, &seq).is_some());

    let packet = outbound_packet(&net, &sender, 400, seq);
    net.router.acknowledge_packet(
        &packet,
        &vec![&net.env, Bytes::from_slice(&net.env, &[SUCCESS_ACK_BYTE])],
        &Bytes::from_slice(&net.env, b"proof"),
        &10,
    );

    assert_eq!(net.transfer.balance_of(&sender, &xlm(&net.env)), 600);
    assert_eq!(
        net.transfer.balance_of(&net.transfer_addr, &xlm(&net.env)),
        400
    );
    assert!(net.router.packet_commitment(&net.client_id, &seq).is_none());
}

#[test]
fn cosmos_to_stellar_recv_credits_receiver_and_writes_ack() {
    let net = setup();
    register_voucher(&net);
    net.env.ledger().set_timestamp(1_000);
    let receiver = Address::generate(&net.env);

    let packet = inbound_packet(&net, &receiver, 500, 1);
    net.router
        .recv_packet(&packet, &Bytes::from_slice(&net.env, b"proof"), &10);

    assert_eq!(
        net.transfer.balance_of(&receiver, &voucher_denom(&net)),
        500
    );
    assert!(net.router.acknowledgement(&net.client_id, &1).is_some());
    assert!(net.router.has_packet_receipt(&net.client_id, &1));
}

#[test]
fn multi_packet_recv_distinct_sequences_all_credit() {
    let net = setup();
    register_voucher(&net);
    net.env.ledger().set_timestamp(1_000);
    let r1 = Address::generate(&net.env);
    let r2 = Address::generate(&net.env);
    let r3 = Address::generate(&net.env);

    let mut seq = 0u64;
    for (receiver, amount) in [(&r1, 100i128), (&r2, 200i128), (&r3, 300i128)] {
        seq += 1;
        let packet = inbound_packet(&net, receiver, amount, seq);
        net.router
            .recv_packet(&packet, &Bytes::from_slice(&net.env, b"proof"), &10);
        assert!(net.router.has_packet_receipt(&net.client_id, &seq));
    }

    assert_eq!(net.transfer.balance_of(&r1, &voucher_denom(&net)), 100);
    assert_eq!(net.transfer.balance_of(&r2, &voucher_denom(&net)), 200);
    assert_eq!(net.transfer.balance_of(&r3, &voucher_denom(&net)), 300);
}

#[test]
fn duplicate_recv_packet_is_rejected() {
    let net = setup();
    register_voucher(&net);
    net.env.ledger().set_timestamp(1_000);
    let receiver = Address::generate(&net.env);
    let packet = inbound_packet(&net, &receiver, 500, 1);

    net.router
        .recv_packet(&packet, &Bytes::from_slice(&net.env, b"proof"), &10);
    let replayed = net
        .router
        .try_recv_packet(&packet, &Bytes::from_slice(&net.env, b"proof"), &10);

    assert!(replayed.is_err());
    assert_eq!(
        net.transfer.balance_of(&receiver, &voucher_denom(&net)),
        500
    );
}

#[test]
fn packet_commitment_present_after_send_absent_for_unknown() {
    let net = setup();
    let sac = register_local(&net);
    let sender = Address::generate(&net.env);
    fund(&net, &sac, &sender, 1_000);

    let seq = net.transfer.initiate_transfer(
        &sender,
        &net.client_id,
        &xlm(&net.env),
        &250,
        &String::from_str(&net.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&net.env, ""),
    );

    assert!(net.router.packet_commitment(&net.client_id, &seq).is_some());
    assert!(net.router.packet_commitment(&net.client_id, &999).is_none());
}
