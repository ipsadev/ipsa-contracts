#![cfg(test)]

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use soroban_sdk::{xdr::ToXdr, Bytes, BytesN, Env, String, Vec};

struct Attestation {
    signing: SigningKey,
}

impl Attestation {
    fn new() -> Self {
        let mut csprng = OsRng;
        Self {
            signing: SigningKey::generate(&mut csprng),
        }
    }
    fn pubkey_bytes(&self, env: &Env) -> BytesN<32> {
        BytesN::from_array(env, &self.signing.verifying_key().to_bytes())
    }
    fn sign(&self, env: &Env, msg: &Bytes) -> BytesN<64> {
        let len = msg.len() as usize;
        let mut buf = [0u8; 4096];
        msg.copy_into_slice(&mut buf[..len]);
        let sig = self.signing.sign(&buf[..len]);
        BytesN::from_array(env, &sig.to_bytes())
    }
}

const N_ATTESTORS: usize = 3;

fn setup(
    min_required_sigs: u32,
) -> (
    Env,
    AttestationLightClientClient<'static>,
    [Attestation; N_ATTESTORS],
) {
    let env = Env::default();
    let contract_id = env.register(AttestationLightClient, ());
    let client = AttestationLightClientClient::new(&env, &contract_id);

    let attestors: [Attestation; N_ATTESTORS] =
        [Attestation::new(), Attestation::new(), Attestation::new()];
    let mut keys: Vec<BytesN<32>> = Vec::new(&env);
    for a in &attestors {
        keys.push_back(a.pubkey_bytes(&env));
    }

    let client_state = AttestorClientState {
        attestor_keys: keys,
        min_required_sigs,
        latest_height: 0,
        frozen: false,
    };
    let cs_bytes = client_state.to_xdr(&env);
    let init_consensus = StateAttestation {
        height: 0,
        timestamp: 0,
    }
    .to_xdr(&env);

    let client_id = String::from_str(&env, "10-attestation-0");
    client.initialise(&client_id, &cs_bytes, &init_consensus, &0);

    (env, client, attestors)
}

fn build_proof_state(
    env: &Env,
    attestors: &[&Attestation],
    indices: &[u32],
    height: u64,
    timestamp: u64,
) -> Bytes {
    let att = StateAttestation { height, timestamp };
    build_proof_for_data(env, attestors, indices, att.to_xdr(env))
}

fn build_proof_packets(
    env: &Env,
    attestors: &[&Attestation],
    indices: &[u32],
    height: u64,
    packets: Vec<PacketCompact>,
) -> Bytes {
    let att = PacketAttestation { height, packets };
    build_proof_for_data(env, attestors, indices, att.to_xdr(env))
}

fn build_proof_for_data(
    env: &Env,
    attestors: &[&Attestation],
    indices: &[u32],
    attestation_data: Bytes,
) -> Bytes {
    let mut signatures: Vec<BytesN<64>> = Vec::new(env);
    let mut signer_indices: Vec<u32> = Vec::new(env);
    for (i, a) in attestors.iter().enumerate() {
        signatures.push_back(a.sign(env, &attestation_data));
        signer_indices.push_back(indices[i]);
    }
    let proof = AttestationProof {
        attestation_data,
        signatures,
        signer_indices,
    };
    proof.to_xdr(env)
}

#[test]
fn initialise_stores_client_state_and_consensus_state() {
    let (env, client, _attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");
    assert_eq!(client.latest_height(&id), 0);
    assert_eq!(client.get_timestamp_at_height(&id, &0), 0);
    assert!(!client.frozen(&id));
}

#[test]
fn update_state_advances_height_with_quorum() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let proof = build_proof_state(&env, &[&attestors[0], &attestors[1]], &[0, 1], 5, 1_000);
    let new_h = client.update_state(&id, &proof);

    assert_eq!(new_h, 5);
    assert_eq!(client.latest_height(&id), 5);
    assert_eq!(client.get_timestamp_at_height(&id, &5), 1_000);
}

#[test]
fn update_state_rejects_below_quorum() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let proof = build_proof_state(&env, &[&attestors[0]], &[0], 5, 1_000);
    let result = client.try_update_state(&id, &proof);
    assert_eq!(result, Err(Ok(Error::QuorumNotMet.into())));
}

#[test]
#[should_panic]
fn update_state_rejects_wrong_signer() {
    let (env, client, attestors) = setup(1);
    let id = String::from_str(&env, "10-attestation-0");

    let att = StateAttestation {
        height: 5,
        timestamp: 1_000,
    };
    let attestation_data = att.to_xdr(&env);
    let signature = attestors[0].sign(&env, &attestation_data);

    let mut signatures: Vec<BytesN<64>> = Vec::new(&env);
    signatures.push_back(signature);
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(1);

    let proof = AttestationProof {
        attestation_data,
        signatures,
        signer_indices: indices,
    }
    .to_xdr(&env);

    client.update_state(&id, &proof);
}

#[test]
fn update_state_rejects_duplicate_signer() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let proof = build_proof_state(&env, &[&attestors[0], &attestors[0]], &[0, 0], 5, 1_000);
    let result = client.try_update_state(&id, &proof);
    assert_eq!(result, Err(Ok(Error::DuplicateSigner.into())));
}

#[test]
fn update_state_rejects_signer_out_of_range() {
    let (env, client, attestors) = setup(1);
    let id = String::from_str(&env, "10-attestation-0");

    let proof = build_proof_state(&env, &[&attestors[0]], &[99], 5, 1_000);
    let result = client.try_update_state(&id, &proof);
    assert_eq!(result, Err(Ok(Error::SignerIndexOutOfRange.into())));
}

#[test]
fn check_for_misbehaviour_detects_conflicting_timestamp() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let p1 = build_proof_state(&env, &[&attestors[0], &attestors[1]], &[0, 1], 5, 1_000);
    client.update_state(&id, &p1);

    let p2 = build_proof_state(&env, &[&attestors[0], &attestors[2]], &[0, 2], 5, 9_999);
    assert!(client.check_for_misbehaviour(&id, &p2));
}

#[test]
fn check_for_misbehaviour_idempotent_for_same_timestamp() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let p1 = build_proof_state(&env, &[&attestors[0], &attestors[1]], &[0, 1], 5, 1_000);
    client.update_state(&id, &p1);

    let p2 = build_proof_state(&env, &[&attestors[0], &attestors[2]], &[0, 2], 5, 1_000);
    assert!(!client.check_for_misbehaviour(&id, &p2));
}

#[test]
fn update_state_on_misbehaviour_freezes_client() {
    let (env, client, _attestors) = setup(1);
    let id = String::from_str(&env, "10-attestation-0");
    assert!(!client.frozen(&id));

    client.update_state_on_misbehaviour(&id, &Bytes::new(&env));

    assert!(client.frozen(&id));
}

#[test]
fn update_state_rejected_once_frozen() {
    let (env, client, attestors) = setup(1);
    let id = String::from_str(&env, "10-attestation-0");

    client.update_state_on_misbehaviour(&id, &Bytes::new(&env));
    let proof = build_proof_state(&env, &[&attestors[0]], &[0], 5, 1_000);
    let result = client.try_update_state(&id, &proof);
    assert_eq!(result, Err(Ok(Error::ClientFrozen.into())));
}

#[test]
fn verify_membership_accepts_attested_packet() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let state_proof = build_proof_state(&env, &[&attestors[0], &attestors[1]], &[0, 1], 5, 1_000);
    client.update_state(&id, &state_proof);

    let path = Bytes::from_slice(&env, b"ibc/commitments/.../sequence/1");
    let value = Bytes::from_slice(&env, b"\xaa\xbb\xcc");
    let pkt = PacketCompact {
        path: path.clone(),
        value: value.clone(),
    };
    let mut packets: Vec<PacketCompact> = Vec::new(&env);
    packets.push_back(pkt);

    let proof = build_proof_packets(&env, &[&attestors[0], &attestors[1]], &[0, 1], 5, packets);
    assert!(client.verify_membership(&id, &5, &proof, &path, &value));
}

#[test]
fn verify_membership_rejects_unknown_height() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let packets: Vec<PacketCompact> = Vec::new(&env);
    let proof = build_proof_packets(&env, &[&attestors[0], &attestors[1]], &[0, 1], 5, packets);
    let path = Bytes::from_slice(&env, b"x");
    let value = Bytes::from_slice(&env, b"y");
    assert!(!client.verify_membership(&id, &5, &proof, &path, &value));
}

#[test]
fn verify_non_membership_accepts_attested_absence() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let state_proof = build_proof_state(&env, &[&attestors[0], &attestors[1]], &[0, 1], 7, 2_000);
    client.update_state(&id, &state_proof);

    let path = Bytes::from_slice(&env, b"ibc/receipts/.../sequence/42");
    let pkt = PacketCompact {
        path: path.clone(),
        value: Bytes::new(&env),
    };
    let mut packets: Vec<PacketCompact> = Vec::new(&env);
    packets.push_back(pkt);

    let proof = build_proof_packets(&env, &[&attestors[0], &attestors[1]], &[0, 1], 7, packets);
    assert!(client.verify_non_membership(&id, &7, &proof, &path));
}

#[test]
fn verify_non_membership_rejects_when_value_present() {
    let (env, client, attestors) = setup(2);
    let id = String::from_str(&env, "10-attestation-0");

    let state_proof = build_proof_state(&env, &[&attestors[0], &attestors[1]], &[0, 1], 7, 2_000);
    client.update_state(&id, &state_proof);

    let path = Bytes::from_slice(&env, b"present-path");
    let pkt = PacketCompact {
        path: path.clone(),
        value: Bytes::from_slice(&env, b"\x01"),
    };
    let mut packets: Vec<PacketCompact> = Vec::new(&env);
    packets.push_back(pkt);

    let proof = build_proof_packets(&env, &[&attestors[0], &attestors[1]], &[0, 1], 7, packets);
    assert!(!client.verify_non_membership(&id, &7, &proof, &path));
}
