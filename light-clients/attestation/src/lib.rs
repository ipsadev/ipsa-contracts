#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error,
    xdr::{FromXdr, ToXdr},
    Bytes, BytesN, Env, String, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    ClientNotInitialised = 1,
    ClientFrozen = 2,
    InvalidClientState = 3,
    InvalidProof = 4,
    SignerIndexOutOfRange = 5,
    DuplicateSigner = 6,
    QuorumNotMet = 7,
    AttestationHeightMismatch = 8,
    ConsensusStateMissing = 9,
}

#[contracttype]
#[derive(Clone)]
pub struct AttestorClientState {
    pub attestor_keys: Vec<BytesN<32>>,
    pub min_required_sigs: u32,
    pub latest_height: u64,
    pub frozen: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct StateAttestation {
    pub height: u64,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PacketCompact {
    pub path: Bytes,
    pub value: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct PacketAttestation {
    pub height: u64,
    pub packets: Vec<PacketCompact>,
}

#[contracttype]
#[derive(Clone)]
pub struct AttestationProof {
    pub attestation_data: Bytes,
    pub signatures: Vec<BytesN<64>>,
    pub signer_indices: Vec<u32>,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Client(String),
    Consensus(String, u64),
    Frozen(String),
}

#[contract]
pub struct AttestationLightClient;

#[contractimpl]
impl AttestationLightClient {
    pub fn initialise(
        env: Env,
        client_id: String,
        client_state: Bytes,
        consensus_state: Bytes,
        height: u64,
    ) {
        let cs = AttestorClientState::from_xdr(&env, &client_state)
            .unwrap_or_else(|_| panic_with_error!(&env, Error::InvalidClientState));

        let timestamp = decode_initial_timestamp(&env, &consensus_state);

        env.storage()
            .persistent()
            .set(&DataKey::Client(client_id.clone()), &cs);
        env.storage()
            .persistent()
            .set(&DataKey::Consensus(client_id, height), &timestamp);
    }

    pub fn latest_height(env: Env, client_id: String) -> u64 {
        load_client_state(&env, &client_id).latest_height
    }

    pub fn client_state(env: Env, client_id: String) -> Bytes {
        load_client_state(&env, &client_id).to_xdr(&env)
    }

    pub fn consensus_state(env: Env, client_id: String, height: u64) -> Bytes {
        let ts: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Consensus(client_id, height))
            .unwrap_or_else(|| panic_with_error!(&env, Error::ConsensusStateMissing));
        StateAttestation {
            height,
            timestamp: ts,
        }
        .to_xdr(&env)
    }

    pub fn verify_client_message(
        _env: Env,
        _client_id: String,
        _client_message: Bytes,
    ) -> (Bytes, u64) {
        (Bytes::new(&_env), 0)
    }

    pub fn check_for_misbehaviour(env: Env, client_id: String, client_message: Bytes) -> bool {
        let cs = load_client_state(&env, &client_id);
        if cs.frozen {
            return false;
        }
        let proof = decode_proof(&env, &client_message);
        verify_quorum(&env, &cs, &proof);

        let att: StateAttestation = decode_attestation(&env, &proof.attestation_data);
        let key = DataKey::Consensus(client_id, att.height);
        match env.storage().persistent().get::<DataKey, u64>(&key) {
            Some(existing) => existing != att.timestamp,
            None => false,
        }
    }

    pub fn update_state(env: Env, client_id: String, client_message: Bytes) -> u64 {
        let mut cs = load_client_state(&env, &client_id);
        if cs.frozen {
            panic_with_error!(&env, Error::ClientFrozen);
        }
        let proof = decode_proof(&env, &client_message);
        verify_quorum(&env, &cs, &proof);

        let att: StateAttestation = decode_attestation(&env, &proof.attestation_data);

        env.storage().persistent().set(
            &DataKey::Consensus(client_id.clone(), att.height),
            &att.timestamp,
        );

        if att.height > cs.latest_height {
            cs.latest_height = att.height;
            env.storage()
                .persistent()
                .set(&DataKey::Client(client_id), &cs);
        }
        att.height
    }

    pub fn update_state_on_misbehaviour(env: Env, client_id: String, _client_message: Bytes) {
        let mut cs = load_client_state(&env, &client_id);
        cs.frozen = true;
        env.storage()
            .persistent()
            .set(&DataKey::Client(client_id.clone()), &cs);
        env.storage()
            .persistent()
            .set(&DataKey::Frozen(client_id), &true);
    }

    pub fn frozen(env: Env, client_id: String) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Frozen(client_id))
            .unwrap_or(false)
    }

    pub fn verify_membership(
        env: Env,
        client_id: String,
        height: u64,
        proof: Bytes,
        path: Bytes,
        value: Bytes,
    ) -> bool {
        let cs = load_client_state(&env, &client_id);
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Consensus(client_id, height))
        {
            return false;
        }
        let p = decode_proof(&env, &proof);
        verify_quorum(&env, &cs, &p);
        let att: PacketAttestation = decode_packet_attestation(&env, &p.attestation_data);
        if att.height != height {
            return false;
        }
        for pkt in att.packets.iter() {
            if pkt.path == path && pkt.value == value {
                return true;
            }
        }
        false
    }

    pub fn verify_non_membership(
        env: Env,
        client_id: String,
        height: u64,
        proof: Bytes,
        path: Bytes,
    ) -> bool {
        let cs = load_client_state(&env, &client_id);
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Consensus(client_id, height))
        {
            return false;
        }
        let p = decode_proof(&env, &proof);
        verify_quorum(&env, &cs, &p);
        let att: PacketAttestation = decode_packet_attestation(&env, &p.attestation_data);
        if att.height != height {
            return false;
        }
        let empty = Bytes::new(&env);
        for pkt in att.packets.iter() {
            if pkt.path == path && pkt.value == empty {
                return true;
            }
        }
        false
    }

    pub fn get_timestamp_at_height(env: Env, client_id: String, height: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Consensus(client_id, height))
            .unwrap_or(0)
    }
}

fn load_client_state(env: &Env, client_id: &String) -> AttestorClientState {
    env.storage()
        .persistent()
        .get(&DataKey::Client(client_id.clone()))
        .unwrap_or_else(|| panic_with_error!(env, Error::ClientNotInitialised))
}

fn decode_proof(env: &Env, bytes: &Bytes) -> AttestationProof {
    AttestationProof::from_xdr(env, bytes)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidProof))
}

fn decode_attestation(env: &Env, bytes: &Bytes) -> StateAttestation {
    StateAttestation::from_xdr(env, bytes)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidProof))
}

fn decode_packet_attestation(env: &Env, bytes: &Bytes) -> PacketAttestation {
    PacketAttestation::from_xdr(env, bytes)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidProof))
}

fn decode_initial_timestamp(env: &Env, bytes: &Bytes) -> u64 {
    StateAttestation::from_xdr(env, bytes)
        .map(|sa| sa.timestamp)
        .unwrap_or(0)
}

fn verify_quorum(env: &Env, cs: &AttestorClientState, proof: &AttestationProof) {
    if proof.signatures.len() != proof.signer_indices.len() {
        panic_with_error!(env, Error::InvalidProof);
    }
    if proof.signatures.len() < cs.min_required_sigs {
        panic_with_error!(env, Error::QuorumNotMet);
    }

    let mut seen = [false; 64];
    let key_count = cs.attestor_keys.len();
    for i in 0..proof.signer_indices.len() {
        let idx = proof.signer_indices.get(i).unwrap();
        if idx >= key_count {
            panic_with_error!(env, Error::SignerIndexOutOfRange);
        }
        let slot = idx as usize;
        if slot >= seen.len() || seen[slot] {
            panic_with_error!(env, Error::DuplicateSigner);
        }
        seen[slot] = true;

        let pubkey = cs.attestor_keys.get(idx).unwrap();
        let signature = proof.signatures.get(i).unwrap();
        env.crypto()
            .ed25519_verify(&pubkey, &proof.attestation_data, &signature);
    }
}

mod test;
