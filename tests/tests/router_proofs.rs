#[path = "../../crates/ics23/tests/common/mod.rs"]
#[allow(dead_code)]
mod common;

use common::{exist_cp, iavl_tree, merkle_bytes, multistore};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, String, Vec,
};
use stellar_ibc_router::{ics24, Error, IbcRouter, IbcRouterClient, Packet, Payload};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_json, FungibleTokenPacketData, IbcTransferApp,
    IbcTransferAppClient, Token,
};
use stellar_tendermint_light_client::{
    Height, TendermintClientState, TendermintConsensusState, TendermintLightClient, TrustThreshold,
};

const HEIGHT: u64 = 10;
const COUNTERPARTY: &str = "08-wasm-0";

struct Net {
    env: Env,
    router: IbcRouterClient<'static>,
    transfer: IbcTransferAppClient<'static>,
    client_id: String,
}

fn client_state(env: &Env) -> Bytes {
    TendermintClientState {
        chain_id: String::from_str(env, "test-chain-0"),
        trust_level: TrustThreshold {
            numerator: 1,
            denominator: 3,
        },
        trusting_period_secs: 1_000_000,
        unbonding_period_secs: 2_000_000,
        max_clock_drift_secs: 10,
        latest_height: Height {
            revision_number: 0,
            revision_height: HEIGHT,
        },
        is_frozen: false,
        frozen_height: Height {
            revision_number: 0,
            revision_height: 0,
        },
        proof_specs: Bytes::new(env),
    }
    .to_xdr(env)
}

fn consensus_state(env: &Env, root: [u8; 32]) -> Bytes {
    TendermintConsensusState {
        timestamp_secs: 1,
        next_validators_hash: BytesN::from_array(env, &[0u8; 32]),
        root: BytesN::from_array(env, &root),
    }
    .to_xdr(env)
}

fn setup(env: Env, root: [u8; 32], prefix: &[&[u8]]) -> Net {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let router_addr = env.register(IbcRouter, (Address::generate(&env),));
    let router = IbcRouterClient::new(&env, &router_addr);

    let client_type = String::from_str(&env, "07-tendermint");
    let light_client = env.register(TendermintLightClient, (router_addr.clone(),));
    router.register_client_type(&client_type, &light_client);

    let client_id = router.create_client(
        &client_type,
        &client_state(&env),
        &consensus_state(&env, root),
        &HEIGHT,
    );

    let mut segments: Vec<Bytes> = Vec::new(&env);
    for segment in prefix {
        segments.push_back(Bytes::from_slice(&env, segment));
    }
    router.register_counterparty(&client_id, &String::from_str(&env, COUNTERPARTY), &segments);

    let transfer_addr = env.register(
        IbcTransferApp,
        (router_addr.clone(), Address::generate(&env)),
    );
    let transfer = IbcTransferAppClient::new(&env, &transfer_addr);
    router.register_port(&String::from_str(&env, "transfer"), &transfer_addr);

    let voucher = env
        .register_stellar_asset_contract_v2(transfer_addr.clone())
        .address();
    transfer.register_asset(&voucher_denom(&env, &client_id), &voucher, &true);

    Net {
        env,
        router,
        transfer,
        client_id,
    }
}

fn voucher_denom(env: &Env, client_id: &String) -> String {
    let mut traced = std::string::String::from("transfer/");

    traced.push_str(&client_id.to_string());
    traced.push_str("/XLM");

    String::from_str(env, &traced)
}

fn inbound_packet(env: &Env, client_id: &String, receiver: &Address) -> Packet {
    let data = FungibleTokenPacketData {
        token: Token {
            denom: String::from_str(env, "XLM"),
            amount: 500,
        },
        sender: String::from_str(env, "cosmos1abc"),
        receiver: address_to_string(env, receiver),
        memo: String::from_str(env, ""),
    };

    Packet {
        sequence: 1,
        source_client: String::from_str(env, COUNTERPARTY),
        dest_client: client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            Payload {
                source_port: String::from_str(env, "transfer"),
                dest_port: String::from_str(env, "transfer"),
                version: String::from_str(env, "ics20-1"),
                encoding: String::from_str(env, "application/json"),
                value: encode_ics20_json(env, &data),
            },
        ],
    }
}

struct CommitmentProof {
    root: [u8; 32],
    proof: std::vec::Vec<u8>,
}

fn commitment_proof(env: &Env, packet: &Packet) -> CommitmentProof {
    let key = ics24::packet_commitment_path(env, &packet.source_client, packet.sequence);
    let mut key_bytes = std::vec![0u8; key.len() as usize];
    key.copy_into_slice(&mut key_bytes);

    let commitment = ics24::commit_v2_packet(env, packet).to_array();

    let tree = iavl_tree(&[
        (&key_bytes, &commitment),
        (
            b"08-wasm-0\x01\x00\x00\x00\x00\x00\x00\x00\x02",
            &[0x02; 32],
        ),
        (
            b"08-wasm-0\x01\x00\x00\x00\x00\x00\x00\x00\x03",
            &[0x03; 32],
        ),
        (
            b"08-wasm-0\x01\x00\x00\x00\x00\x00\x00\x00\x04",
            &[0x04; 32],
        ),
    ]);
    let store = multistore(tree.root);

    CommitmentProof {
        root: store.root,
        proof: merkle_bytes(&[exist_cp(tree.proofs[0].clone()), exist_cp(store.proof)]),
    }
}

fn prepared(prefix: &[&[u8]]) -> (Net, Packet, Bytes, Address) {
    let env = Env::default();

    let receiver = Address::generate(&env);
    let packet = inbound_packet(&env, &String::from_str(&env, "07-tendermint-0"), &receiver);
    let proof = commitment_proof(&env, &packet);
    let proof_bytes = Bytes::from_slice(&env, &proof.proof);

    let net = setup(env, proof.root, prefix);
    assert_eq!(net.client_id, packet.dest_client);

    (net, packet, proof_bytes, receiver)
}

#[test]
fn a_real_commitment_proof_is_received_under_the_ibc_go_prefix() {
    let (net, packet, proof, receiver) = prepared(&[b"ibc", b""]);

    net.router.recv_packet(&packet, &proof, &HEIGHT);

    assert!(net.router.has_packet_receipt(&net.client_id, &1));
    assert_eq!(
        net.transfer
            .balance_of(&receiver, &voucher_denom(&net.env, &net.client_id)),
        500
    );
}

#[test]
fn a_prefix_that_splits_the_store_key_is_refused() {
    let (net, packet, proof, _) = prepared(&[b"ib", b"c"]);

    assert_eq!(
        net.router.try_recv_packet(&packet, &proof, &HEIGHT),
        Err(Ok(Error::MembershipVerificationFailed.into()))
    );
}

#[test]
fn a_single_segment_prefix_is_refused_for_a_two_level_client() {
    let env = Env::default();
    env.mock_all_auths();

    let router_addr = env.register(IbcRouter, (Address::generate(&env),));
    let router = IbcRouterClient::new(&env, &router_addr);

    let client_type = String::from_str(&env, "07-tendermint");
    let light_client = env.register(TendermintLightClient, (router_addr.clone(),));
    router.register_client_type(&client_type, &light_client);

    let client_id = router.create_client(
        &client_type,
        &client_state(&env),
        &consensus_state(&env, [0u8; 32]),
        &HEIGHT,
    );

    assert_eq!(
        router.try_register_counterparty(
            &client_id,
            &String::from_str(&env, COUNTERPARTY),
            &vec![&env, Bytes::from_slice(&env, b"ibc")],
        ),
        Err(Ok(Error::InvalidCommitmentPrefix.into()))
    );
}
