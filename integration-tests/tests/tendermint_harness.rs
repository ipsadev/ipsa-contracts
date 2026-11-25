use integration_tests::router::{make_payload, EchoApp};
use integration_tests::tendermint::{commit_packets, register, COUNTERPARTY, HEIGHT, NOW};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, Env, String, Vec,
};
use stellar_ibc_router::{IbcRouter, IbcRouterClient, Packet};

const LOCAL_CLIENT_ID: &str = "07-tendermint-0";

fn inbound(env: &Env, sequence: u64, value: &[u8]) -> Packet {
    Packet {
        sequence,
        source_client: String::from_str(env, COUNTERPARTY),
        dest_client: String::from_str(env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            make_payload(env, &String::from_str(env, "transfer"), value),
        ],
    }
}

struct Net {
    env: Env,
    router: IbcRouterClient<'static>,
    client_id: String,
    proofs: std::vec::Vec<Bytes>,
}

fn net_committing(packets: &[Packet], env: Env) -> Net {
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = NOW);

    let router_id = env.register(IbcRouter, (Address::generate(&env),));
    let router = IbcRouterClient::new(&env, &router_id);

    let committed = commit_packets(&env, packets);
    let (_light_client, client_id) = register(&env, &router, "07-tendermint", committed.root, 1);

    assert_eq!(
        client_id,
        String::from_str(&env, LOCAL_CLIENT_ID),
        "the packets were committed against this client id"
    );

    let prefix: Vec<Bytes> = vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)];
    router.register_counterparty(&client_id, &String::from_str(&env, COUNTERPARTY), &prefix);

    let app = env.register(EchoApp, ());
    router.register_port(&String::from_str(&env, "transfer"), &app);

    Net {
        env,
        router,
        client_id,
        proofs: committed.proofs,
    }
}

#[test]
fn the_real_client_accepts_a_proof_the_harness_built() {
    let env = Env::default();
    let packet = inbound(&env, 1, b"hello");
    let net = net_committing(std::slice::from_ref(&packet), env);

    net.router.recv_packet(&packet, &net.proofs[0], &HEIGHT);

    assert!(net.router.has_packet_receipt(&net.client_id, &1));
}

#[test]
fn several_packets_share_one_committed_root() {
    let env = Env::default();
    let packets = [
        inbound(&env, 1, b"one"),
        inbound(&env, 2, b"two"),
        inbound(&env, 3, b"three"),
    ];
    let net = net_committing(&packets, env);

    for (i, packet) in packets.iter().enumerate() {
        net.router.recv_packet(packet, &net.proofs[i], &HEIGHT);

        assert!(net
            .router
            .has_packet_receipt(&net.client_id, &packet.sequence));
    }
}

#[test]
#[should_panic]
fn the_real_client_refuses_a_proof_for_another_packet() {
    let env = Env::default();
    let packets = [inbound(&env, 1, b"one"), inbound(&env, 2, b"two")];
    let net = net_committing(&packets, env);

    net.router.recv_packet(&packets[0], &net.proofs[1], &HEIGHT);
}

#[test]
#[should_panic]
fn the_real_client_refuses_a_garbage_proof() {
    let env = Env::default();
    let packet = inbound(&env, 1, b"hello");
    let net = net_committing(std::slice::from_ref(&packet), env);
    let garbage = Bytes::from_slice(&net.env, b"proof");

    net.router.recv_packet(&packet, &garbage, &HEIGHT);
}
