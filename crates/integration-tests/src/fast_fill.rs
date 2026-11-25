#![allow(dead_code)]

use crate::tendermint::{commit_entries, register, Entry, HEIGHT};
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    vec, Address, Bytes, BytesN, Env, String,
};
use stellar_ibc_fast_fill::{Config, FastFill, FastFillClient, Order};
use stellar_ibc_router::{
    ics24::{commit_v2_acknowledgement, error_ack_hash},
    IbcRouter, IbcRouterClient, Packet, Payload,
};
use stellar_ibc_transfer::{
    encode_ics20_json, FungibleTokenPacketData, IbcTransferApp, IbcTransferAppClient, Token,
};

pub const ENCODING: &str = "application/json";

pub const SUCCESS_ACK_JSON: &[u8] = b"{\"result\":\"AQ==\"}";

pub const WETH: &str = "0xfff9976782d46cc05630d1f6ebab18b2324d6b14";

pub const FAST_TRANSFER: &str = "0x1111111111111111111111111111111111111111";

pub const PROVIDER_EVM: &str = "0x2222222222222222222222222222222222222222";

pub const NOW: u64 = 1_000;

pub const AMOUNT: i128 = 1_000_000;

pub const FEE: i128 = 2_500;

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SentCall {
    pub sender: Address,
    pub source_client_id: String,
    pub receiver: String,
    pub salt: Bytes,
    pub payload: Bytes,
    pub timeout_timestamp: u64,
    pub memo: String,
    pub encoding: String,
}

#[contract]
pub struct RecordingGmp;

#[contractimpl]
impl RecordingGmp {
    #[allow(clippy::too_many_arguments)]
    pub fn send_call(
        env: Env,
        sender: Address,
        source_client_id: String,
        receiver: String,
        salt: Bytes,
        payload: Bytes,
        timeout_timestamp: u64,
        memo: String,
        encoding: String,
    ) -> u64 {
        sender.require_auth();

        env.storage().instance().set(
            &symbol_short!("sent"),
            &SentCall {
                sender,
                source_client_id,
                receiver,
                salt,
                payload,
                timeout_timestamp,
                memo,
                encoding,
            },
        );

        7
    }

    pub fn sent(env: Env) -> Option<SentCall> {
        env.storage().instance().get(&symbol_short!("sent"))
    }
}

pub struct Fixture {
    pub proofs: std::vec::Vec<Bytes>,
    pub env: Env,
    pub router: IbcRouterClient<'static>,
    pub transfer: IbcTransferAppClient<'static>,
    pub transfer_addr: Address,
    pub fast_fill: FastFillClient<'static>,
    pub fast_fill_addr: Address,
    pub gmp: RecordingGmpClient<'static>,
    pub stellar_client: String,
    pub sepolia_client: String,
    pub voucher: Address,
    pub provider: Address,
    pub recipient: Address,
}

pub const LOCAL_CLIENT_ID: &str = "test-0";

pub fn setup() -> Fixture {
    setup_with(Env::default(), |_, _, _| std::vec::Vec::new())
}

pub fn setup_with<F>(env: Env, entries: F) -> Fixture
where
    F: FnOnce(&Env, &Address, &Address) -> std::vec::Vec<Entry>,
{
    env.mock_all_auths();
    env.ledger().set_timestamp(NOW);

    let router_addr = env.register(IbcRouter, (Address::generate(&env),));
    let router = IbcRouterClient::new(&env, &router_addr);
    let stellar_client = String::from_str(&env, LOCAL_CLIENT_ID);
    let sepolia_client = String::from_str(&env, "sp1-stellar-0");

    let transfer_addr = env.register(
        IbcTransferApp,
        (router_addr.clone(), Address::generate(&env)),
    );

    let transfer = IbcTransferAppClient::new(&env, &transfer_addr);

    router.register_port(&String::from_str(&env, "transfer"), &transfer_addr);

    let voucher = env
        .register_stellar_asset_contract_v2(transfer_addr.clone())
        .address();

    transfer.register_asset(&traced(&env, &stellar_client, WETH), &voucher, &true);

    let gmp_addr = env.register(RecordingGmp, ());
    let gmp = RecordingGmpClient::new(&env, &gmp_addr);

    let fast_fill_addr = env.register(
        FastFill,
        (
            Address::generate(&env),
            Config {
                transfer_app: transfer_addr.clone(),
                gmp_app: gmp_addr,
                notice_encoding: String::from_str(&env, "application/x-solidity-abi"),
                notice_timeout_seconds: 3_600,
            },
        ),
    );

    let fast_fill = FastFillClient::new(&env, &fast_fill_addr);

    fast_fill.set_route(&stellar_client, &String::from_str(&env, WETH), &voucher);

    let provider = Address::generate(&env);
    let recipient = Address::generate(&env);

    StellarAssetClient::new(&env, &voucher).mint(&provider, &(10 * AMOUNT));

    let committed = commit_entries(&env, &entries(&env, &fast_fill_addr, &recipient));
    let (_light_client, created) = register(&env, &router, "test", committed.root, 1);

    assert_eq!(created, stellar_client, "the entries assume this client id");

    router.register_counterparty(
        &stellar_client,
        &sepolia_client,
        &vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)],
    );

    Fixture {
        env,
        router,
        transfer,
        transfer_addr,
        fast_fill,
        fast_fill_addr,
        gmp,
        stellar_client,
        sepolia_client,
        voucher,
        provider,
        recipient,
        proofs: committed.proofs,
    }
}

pub fn traced(env: &Env, client: &String, denom: &str) -> String {
    String::from_str(env, &format!("transfer/{}/{denom}", client))
}

pub fn memo_for(f: &Fixture, recipient: &Address, fee: i128, deadline: u64) -> String {
    String::from_str(
        &f.env,
        &format!(
            "{{\"dest_callback\":{{\"address\":\"{}\"}},\"fast_fill\":\"{}:{fee}:{deadline}\"}}",
            f.fast_fill_addr.to_string(),
            recipient.to_string()
        ),
    )
}

pub fn order_with(f: &Fixture, sequence: u64, amount: i128, memo: String) -> Order {
    Order {
        source_client: f.sepolia_client.clone(),
        dest_client: f.stellar_client.clone(),
        sequence,
        sender: String::from_str(&f.env, FAST_TRANSFER),
        denom: String::from_str(&f.env, WETH),
        amount,
        memo,
    }
}

pub fn order(f: &Fixture, sequence: u64) -> Order {
    order_with(
        f,
        sequence,
        AMOUNT,
        memo_for(f, &f.recipient, FEE, NOW + 600),
    )
}

pub fn packet(f: &Fixture, order: &Order, receiver: &Address) -> Packet {
    let data = FungibleTokenPacketData {
        token: Token {
            denom: order.denom.clone(),
            amount: order.amount,
        },
        sender: order.sender.clone(),
        receiver: receiver.to_string(),
        memo: order.memo.clone(),
    };

    Packet {
        sequence: order.sequence,
        source_client: order.source_client.clone(),
        dest_client: order.dest_client.clone(),
        timeout_timestamp: NOW + 7_200,
        payloads: vec![
            &f.env,
            Payload {
                source_port: String::from_str(&f.env, "transfer"),
                dest_port: String::from_str(&f.env, "transfer"),
                version: String::from_str(&f.env, "ics20-1"),
                encoding: String::from_str(&f.env, ENCODING),
                value: encode_ics20_json(&f.env, &data),
            },
        ],
    }
}

pub fn deliver(f: &Fixture, order: &Order) {
    f.router.recv_packet(
        &packet(f, order, &f.fast_fill_addr),
        &f.proofs[0].clone(),
        &HEIGHT,
    );
}

pub fn balance(f: &Fixture, who: &Address) -> i128 {
    TokenClient::new(&f.env, &f.voucher).balance(who)
}

pub fn success_ack(f: &Fixture) -> BytesN<32> {
    commit_v2_acknowledgement(
        &f.env,
        &vec![&f.env, Bytes::from_slice(&f.env, SUCCESS_ACK_JSON)],
    )
}

pub fn error_ack(f: &Fixture) -> BytesN<32> {
    let error: Bytes = error_ack_hash(&f.env).into();

    commit_v2_acknowledgement(&f.env, &vec![&f.env, error])
}

pub fn provider_evm(f: &Fixture) -> String {
    String::from_str(&f.env, PROVIDER_EVM)
}

pub fn memo_in(
    env: &Env,
    fast_fill: &Address,
    recipient: &Address,
    fee: i128,
    deadline: u64,
) -> String {
    String::from_str(
        env,
        &format!(
            "{{\"dest_callback\":{{\"address\":\"{}\"}},\"fast_fill\":\"{}:{fee}:{deadline}\"}}",
            fast_fill.to_string(),
            recipient.to_string()
        ),
    )
}

pub fn order_in(env: &Env, sequence: u64, amount: i128, memo: String) -> Order {
    Order {
        source_client: String::from_str(env, "sp1-stellar-0"),
        dest_client: String::from_str(env, LOCAL_CLIENT_ID),
        sequence,
        sender: String::from_str(env, FAST_TRANSFER),
        denom: String::from_str(env, WETH),
        amount,
        memo,
    }
}

pub fn packet_in(env: &Env, order: &Order, receiver: &Address) -> Packet {
    let data = FungibleTokenPacketData {
        token: Token {
            denom: order.denom.clone(),
            amount: order.amount,
        },
        sender: order.sender.clone(),
        receiver: receiver.to_string(),
        memo: order.memo.clone(),
    };

    Packet {
        sequence: order.sequence,
        source_client: order.source_client.clone(),
        dest_client: order.dest_client.clone(),
        timeout_timestamp: NOW + 7_200,
        payloads: vec![
            env,
            Payload {
                source_port: String::from_str(env, "transfer"),
                dest_port: String::from_str(env, "transfer"),
                version: String::from_str(env, "ics20-1"),
                encoding: String::from_str(env, ENCODING),
                value: encode_ics20_json(env, &data),
            },
        ],
    }
}

pub fn setup_delivering(sequence: u64, amount: i128, fee: i128, deadline: u64) -> Fixture {
    setup_with(Env::default(), move |env, fast_fill, recipient| {
        let memo = memo_in(env, fast_fill, recipient, fee, deadline);
        let order = order_in(env, sequence, amount, memo);

        std::vec![Entry::Packet(packet_in(env, &order, fast_fill))]
    })
}

pub fn setup_for_order(sequence: u64) -> Fixture {
    setup_delivering(sequence, AMOUNT, FEE, NOW + 600)
}
