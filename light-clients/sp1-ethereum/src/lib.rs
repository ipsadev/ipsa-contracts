#![no_std]

mod membership;
mod wire;

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, String, Symbol,
};
use stellar_access::access_control::set_admin;
use stellar_macros::only_admin;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Vkey(String),
    MembershipVkey(String),
    ClientStateCommitment(String),
    ConsensusHash(String, u64),
    Timestamp(String, u64),
    LatestSlot(String),
    Frozen(String),
}

#[contract]
pub struct Sp1EthereumLightClient;

#[contractimpl]
impl Sp1EthereumLightClient {
    pub fn __constructor(env: Env, router: Address) {
        set_admin(&env, &router);
    }

    #[only_admin]
    pub fn initialise(
        env: Env,
        client_id: String,
        client_state: Bytes,
        consensus_state: Bytes,
        height: u64,
    ) {
        if client_state.len() != 96 {
            panic!("the client state must be 96 bytes: two verifying keys and a commitment");
        }

        if consensus_state.len() != 40 {
            panic!("the consensus state must be 40 bytes: a hash and a timestamp");
        }

        let storage = env.storage().persistent();

        if storage.has(&DataKey::Vkey(client_id.clone())) {
            panic!("this client id is already initialised");
        }

        storage.set(
            &DataKey::Vkey(client_id.clone()),
            &bytes32(&env, &client_state, 0),
        );
        storage.set(
            &DataKey::MembershipVkey(client_id.clone()),
            &bytes32(&env, &client_state, 32),
        );
        storage.set(
            &DataKey::ClientStateCommitment(client_id.clone()),
            &bytes32(&env, &client_state, 64),
        );
        storage.set(
            &DataKey::ConsensusHash(client_id.clone(), height),
            &bytes32(&env, &consensus_state, 0),
        );

        let mut raw = [0_u8; 8];

        for index in 0..8_u32 {
            raw[index as usize] = consensus_state.get(32 + index).unwrap_or(0);
        }

        storage.set(
            &DataKey::Timestamp(client_id.clone(), height),
            &u64::from_be_bytes(raw),
        );
        storage.set(&DataKey::LatestSlot(client_id), &height);
    }

    pub fn latest_height(env: Env, client_id: String) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestSlot(client_id))
            .unwrap_or(0)
    }

    pub fn frozen(env: Env, client_id: String) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Frozen(client_id))
            .unwrap_or(false)
    }

    pub fn consensus_hash(env: Env, client_id: String, slot: u64) -> BytesN<32> {
        env.storage()
            .persistent()
            .get(&DataKey::ConsensusHash(client_id, slot))
            .unwrap_or_else(|| panic!("no consensus state at that slot"))
    }

    pub fn get_timestamp_at_height(env: Env, client_id: String, slot: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Timestamp(client_id, slot))
            .unwrap_or_else(|| panic!("no consensus state at that slot"))
    }

    pub fn update_state(env: Env, client_id: String, client_message: Bytes) -> u64 {
        if Self::frozen(env.clone(), client_id.clone()) {
            panic!("the client is frozen");
        }

        let (public_values, proof) = split(&client_message);

        let storage = env.storage().persistent();

        let vkey: BytesN<32> = storage
            .get(&DataKey::Vkey(client_id.clone()))
            .unwrap_or_else(|| panic!("this client id is not initialised"));

        if stellar_sp1_verifier::verify_proof(&env, &vkey, &public_values, &proof).is_err() {
            panic!("the proof does not verify");
        }

        let output = wire::decode(&env, &public_values);

        let pinned: BytesN<32> = storage
            .get(&DataKey::ClientStateCommitment(client_id.clone()))
            .unwrap_or_else(|| panic!("this client id is not initialised"));

        if output.client_state_commitment != pinned {
            panic!("the proof was made against different client parameters");
        }

        let trusted: BytesN<32> = storage
            .get(&DataKey::ConsensusHash(
                client_id.clone(),
                output.trusted_slot,
            ))
            .unwrap_or_else(|| panic!("no consensus state at the slot the proof starts from"));

        if output.trusted_consensus_hash != trusted {
            panic!("the proof starts from a consensus state this client does not hold");
        }

        let existing: Option<BytesN<32>> =
            storage.get(&DataKey::ConsensusHash(client_id.clone(), output.new_slot));

        if let Some(held) = existing {
            if held != output.new_consensus_hash {
                panic!("a different consensus state is already stored at that slot");
            }

            return output.new_slot;
        }

        storage.set(
            &DataKey::ConsensusHash(client_id.clone(), output.new_slot),
            &output.new_consensus_hash,
        );
        storage.set(
            &DataKey::Timestamp(client_id.clone(), output.new_slot),
            &output.timestamp,
        );

        if output.new_slot > Self::latest_height(env.clone(), client_id.clone()) {
            storage.set(&DataKey::LatestSlot(client_id), &output.new_slot);
        }

        output.new_slot
    }

    pub fn verify_membership(
        env: Env,
        client_id: String,
        proof_height: u64,
        proof: Bytes,
        path: Bytes,
        value: Bytes,
    ) -> bool {
        Self::prove(env, client_id, proof_height, proof, path, value)
    }

    pub fn verify_non_membership(
        env: Env,
        client_id: String,
        proof_height: u64,
        proof: Bytes,
        path: Bytes,
    ) -> bool {
        let empty = Bytes::new(&env);

        Self::prove(env, client_id, proof_height, proof, path, empty)
    }

    pub fn check_for_misbehaviour(_env: Env, _client_id: String, _client_message: Bytes) -> bool {
        false
    }

    pub fn update_state_on_misbehaviour(_env: Env, _client_id: String, _client_message: Bytes) {
        panic!("misbehaviour handling is not built yet");
    }

    fn prove(
        env: Env,
        client_id: String,
        proof_height: u64,
        proof: Bytes,
        path: Bytes,
        value: Bytes,
    ) -> bool {
        if Self::frozen(env.clone(), client_id.clone()) {
            panic!("the client is frozen");
        }

        let (public_values, proof_bytes) = split(&proof);

        let storage = env.storage().persistent();

        let vkey: BytesN<32> = storage
            .get(&DataKey::MembershipVkey(client_id.clone()))
            .unwrap_or_else(|| panic!("this client id has no membership program"));

        if stellar_sp1_verifier::verify_proof(&env, &vkey, &public_values, &proof_bytes).is_err() {
            panic!("the proof does not verify");
        }

        let output = membership::decode(&env, &public_values);

        let pinned: BytesN<32> = storage
            .get(&DataKey::ClientStateCommitment(client_id.clone()))
            .unwrap_or_else(|| panic!("this client id is not initialised"));

        if output.client_state_commitment != pinned {
            panic!("the proof was made against different client parameters");
        }

        if output.slot != proof_height {
            panic!("the proof is of a different height than the one asked about");
        }

        let held: BytesN<32> = storage
            .get(&DataKey::ConsensusHash(client_id.clone(), proof_height))
            .unwrap_or_else(|| panic!("no consensus state at that slot"));

        if output.consensus_hash != held {
            panic!("the proof is against a consensus state this client does not hold");
        }

        if output.kv_commitment != membership::kv_commitment(&env, &path, &value) {
            panic!("the proof is of a different path or value");
        }

        true
    }

    pub fn client_type(_env: Env) -> Symbol {
        symbol_short!("sp1eth")
    }
}

fn bytes32(env: &Env, source: &Bytes, offset: u32) -> BytesN<32> {
    let mut raw = [0_u8; 32];

    for index in 0..32_u32 {
        raw[index as usize] = source.get(offset + index).unwrap_or(0);
    }

    BytesN::from_array(env, &raw)
}

fn split(client_message: &Bytes) -> (Bytes, Bytes) {
    if client_message.len() < 4 {
        panic!("the client message carries no length prefix");
    }

    let mut prefix = [0_u8; 4];

    for index in 0..4_u32 {
        prefix[index as usize] = client_message.get(index).unwrap_or(0);
    }

    let length = u32::from_be_bytes(prefix);

    if client_message.len() < 4 + length {
        panic!("the client message is shorter than its length prefix claims");
    }

    (
        client_message.slice(4..4 + length),
        client_message.slice(4 + length..client_message.len()),
    )
}
