#![allow(dead_code)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{vec, Address, Bytes, Env, String};
use stellar_ibc_router::{IbcRouter, IbcRouterClient};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_json, FungibleTokenPacketData, IbcTransferApp,
    IbcTransferAppClient, Token,
};
use stellar_mock_light_client::MockLightClient;

pub const ENCODING: &str = "application/json";
pub const SUCCESS_ACK_BYTE: u8 = 0x01;

pub struct Fixture {
    pub env: Env,
    pub router: IbcRouterClient<'static>,
    pub transfer: IbcTransferAppClient<'static>,
    pub transfer_addr: Address,
    pub source_client_id: String,
    pub counterparty_client_id: String,
    pub transfer_admin: Address,
}

pub fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let router_admin = Address::generate(&env);
    let router_addr = env.register(IbcRouter, (router_admin,));
    let router = IbcRouterClient::new(&env, &router_addr);
    let lc_id = env.register(MockLightClient, ());
    router.register_client_type(&String::from_str(&env, "mock"), &lc_id);

    let source_client_id = router.create_client(
        &String::from_str(&env, "mock"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );
    let counterparty_client_id = String::from_str(&env, "07-tendermint-0");
    router.register_counterparty(
        &source_client_id,
        &counterparty_client_id,
        &vec![&env, Bytes::from_slice(&env, b"ibc")],
    );

    let transfer_admin = Address::generate(&env);
    let transfer_addr = env.register(IbcTransferApp, (router_addr, transfer_admin.clone()));
    let transfer = IbcTransferAppClient::new(&env, &transfer_addr);

    router.register_port(&String::from_str(&env, "transfer"), &transfer_addr);

    Fixture {
        env,
        router,
        transfer,
        transfer_addr,
        source_client_id,
        counterparty_client_id,
        transfer_admin,
    }
}

pub fn xlm(env: &Env) -> String {
    String::from_str(env, "XLM")
}

pub fn register_local(f: &Fixture, denom: &String) -> Address {
    let issuer = Address::generate(&f.env);
    let sac = f.env.register_stellar_asset_contract_v2(issuer).address();
    f.transfer.register_asset(denom, &sac, &false);

    sac
}

pub fn register_voucher(f: &Fixture, denom: &String) -> Address {
    let sac = f
        .env
        .register_stellar_asset_contract_v2(f.transfer_addr.clone())
        .address();
    f.transfer.register_asset(denom, &sac, &true);

    sac
}

pub fn fund(f: &Fixture, sac: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(&f.env, sac).mint(to, &amount);
}

pub fn build_inbound_packet(
    env: &Env,
    sender_blob: &str,
    receiver: &Address,
    denom: &String,
    amount: i128,
) -> Bytes {
    let pkt = FungibleTokenPacketData {
        token: Token {
            denom: denom.clone(),
            amount,
        },
        sender: String::from_str(env, sender_blob),
        receiver: address_to_string(env, receiver),
        memo: String::from_str(env, ""),
    };

    encode_ics20_json(env, &pkt)
}
