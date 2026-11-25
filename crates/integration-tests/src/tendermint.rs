use soroban_sdk::{xdr::ToXdr, Address, Bytes, BytesN, Env, String, Vec};
use stellar_ibc_router::{ics24, IbcRouterClient, Packet};
use stellar_tendermint_light_client::{
    Height, TendermintClientState, TendermintConsensusState, TendermintLightClient, TrustThreshold,
};

use crate::ics23::{exist_cp, iavl_tree_n, merkle_bytes, multistore, nonexist_cp};
use ics23::NonExistenceProof;

pub const HEIGHT: u64 = 10;
pub const CLIENT_TYPE: &str = "07-tendermint";
pub const COUNTERPARTY: &str = "08-wasm-0";
pub const NOW: u64 = 1_000;
pub const CONSENSUS_TIMESTAMP: u64 = 1;

pub fn client_state(env: &Env, height: u64) -> Bytes {
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
            revision_height: height,
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

pub fn consensus_state(env: &Env, root: [u8; 32], timestamp_secs: u64) -> Bytes {
    TendermintConsensusState {
        timestamp_secs,
        next_validators_hash: BytesN::from_array(env, &[0u8; 32]),
        root: BytesN::from_array(env, &root),
    }
    .to_xdr(env)
}

pub struct Committed {
    pub root: [u8; 32],
    pub proofs: std::vec::Vec<Bytes>,
}

pub const EMPTY_ROOT: [u8; 32] = [0u8; 32];

fn commit(env: &Env, entries: &[(std::vec::Vec<u8>, [u8; 32])]) -> Committed {
    let leaves: std::vec::Vec<(&[u8], &[u8])> = entries
        .iter()
        .map(|(key, value)| (key.as_slice(), value.as_slice()))
        .collect();

    let tree = iavl_tree_n(&leaves);
    let store = multistore(tree.root);

    let proofs = (0..entries.len())
        .map(|position| {
            Bytes::from_slice(
                env,
                &merkle_bytes(&[
                    exist_cp(tree.proofs[position].clone()),
                    exist_cp(store.proof.clone()),
                ]),
            )
        })
        .collect();

    Committed {
        root: store.root,
        proofs,
    }
}

fn path_bytes(path: &Bytes) -> std::vec::Vec<u8> {
    let mut out = std::vec![0u8; path.len() as usize];
    path.copy_into_slice(&mut out);

    out
}

pub fn commit_receipt_absence(env: &Env, packet: &Packet) -> Committed {
    let absent = path_bytes(&ics24::packet_receipt_path(
        env,
        &packet.dest_client,
        packet.sequence,
    ));
    let below = path_bytes(&ics24::packet_commitment_path(env, &packet.dest_client, 1));
    let above = path_bytes(&ics24::packet_receipt_path(
        env,
        &packet.dest_client,
        packet.sequence + 1,
    ));

    assert!(
        below < absent && absent < above,
        "the neighbours must straddle the absent key"
    );

    let value = [0x01u8];
    let tree = iavl_tree_n(&[(below.as_slice(), &value), (above.as_slice(), &value)]);
    let store = multistore(tree.root);

    let proof = merkle_bytes(&[
        nonexist_cp(NonExistenceProof {
            key: absent,
            left: Some(tree.proofs[0].clone()),
            right: Some(tree.proofs[1].clone()),
        }),
        exist_cp(store.proof.clone()),
    ]);

    Committed {
        root: store.root,
        proofs: std::vec![Bytes::from_slice(env, &proof)],
    }
}

pub fn commit_acknowledgement_and_absence(
    env: &Env,
    packet: &Packet,
    application_acknowledgements: &Vec<Bytes>,
) -> Committed {
    let absent = path_bytes(&ics24::packet_receipt_path(
        env,
        &packet.dest_client,
        packet.sequence,
    ));
    let below = path_bytes(&ics24::packet_commitment_path(env, &packet.dest_client, 1));
    let above = path_bytes(&ics24::packet_receipt_path(
        env,
        &packet.dest_client,
        packet.sequence + 1,
    ));
    let acknowledgement_key = path_bytes(&ics24::ack_commitment_path(
        env,
        &packet.dest_client,
        packet.sequence,
    ));
    let acknowledgement_value =
        ics24::commit_v2_acknowledgement(env, application_acknowledgements).to_array();

    assert!(below < absent && absent < above);

    let one = [0x01u8];
    let tree = iavl_tree_n(&[
        (below.as_slice(), &one),
        (above.as_slice(), &one),
        (acknowledgement_key.as_slice(), &acknowledgement_value),
    ]);
    let store = multistore(tree.root);

    let acknowledgement_proof = merkle_bytes(&[
        exist_cp(tree.proofs[2].clone()),
        exist_cp(store.proof.clone()),
    ]);
    let absence_proof = merkle_bytes(&[
        nonexist_cp(NonExistenceProof {
            key: absent,
            left: Some(tree.proofs[0].clone()),
            right: Some(tree.proofs[1].clone()),
        }),
        exist_cp(store.proof.clone()),
    ]);

    Committed {
        root: store.root,
        proofs: std::vec![
            Bytes::from_slice(env, &acknowledgement_proof),
            Bytes::from_slice(env, &absence_proof),
        ],
    }
}

pub enum Entry {
    Packet(Packet),
    Acknowledgement(Packet, Vec<Bytes>),
}

pub fn commit_entries(env: &Env, entries: &[Entry]) -> Committed {
    if entries.is_empty() {
        return commit(env, &[(b"unused".to_vec(), [0u8; 32])]);
    }

    let keyed: std::vec::Vec<(std::vec::Vec<u8>, [u8; 32])> = entries
        .iter()
        .map(|entry| match entry {
            Entry::Packet(packet) => (
                path_bytes(&ics24::packet_commitment_path(
                    env,
                    &packet.source_client,
                    packet.sequence,
                )),
                ics24::commit_v2_packet(env, packet).to_array(),
            ),
            Entry::Acknowledgement(packet, application_acknowledgements) => (
                path_bytes(&ics24::ack_commitment_path(
                    env,
                    &packet.dest_client,
                    packet.sequence,
                )),
                ics24::commit_v2_acknowledgement(env, application_acknowledgements).to_array(),
            ),
        })
        .collect();

    commit(env, &keyed)
}

pub fn commit_packets(env: &Env, packets: &[Packet]) -> Committed {
    if packets.is_empty() {
        return commit(env, &[(b"unused".to_vec(), [0u8; 32])]);
    }

    let entries: std::vec::Vec<(std::vec::Vec<u8>, [u8; 32])> = packets
        .iter()
        .map(|packet| {
            (
                path_bytes(&ics24::packet_commitment_path(
                    env,
                    &packet.source_client,
                    packet.sequence,
                )),
                ics24::commit_v2_packet(env, packet).to_array(),
            )
        })
        .collect();

    commit(env, &entries)
}

pub fn commit_acknowledgements(env: &Env, acknowledgements: &[(Packet, Vec<Bytes>)]) -> Committed {
    let entries: std::vec::Vec<(std::vec::Vec<u8>, [u8; 32])> = acknowledgements
        .iter()
        .map(|(packet, application_acknowledgements)| {
            (
                path_bytes(&ics24::ack_commitment_path(
                    env,
                    &packet.dest_client,
                    packet.sequence,
                )),
                ics24::commit_v2_acknowledgement(env, application_acknowledgements).to_array(),
            )
        })
        .collect();

    commit(env, &entries)
}

pub fn register(
    env: &Env,
    router: &IbcRouterClient<'static>,
    client_type: &str,
    root: [u8; 32],
    timestamp_secs: u64,
) -> (Address, String) {
    let client_type = String::from_str(env, client_type);
    let light_client = env.register(TendermintLightClient, (router.address.clone(),));

    router.register_client_type(&client_type, &light_client);

    let client_id = router.create_client(
        &client_type,
        &client_state(env, HEIGHT),
        &consensus_state(env, root, timestamp_secs),
        &HEIGHT,
    );

    (light_client, client_id)
}
