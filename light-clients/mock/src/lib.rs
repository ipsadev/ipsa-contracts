#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Bytes, Env, String, Symbol};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    ClientState(String),
    ConsensusState(String, u64),
    LatestHeight(String),
    Frozen(String),
}

#[contract]
pub struct MockLightClient;

#[contractimpl]
impl MockLightClient {
    pub fn initialise(
        env: Env,
        client_id: String,
        client_state: Bytes,
        consensus_state: Bytes,
        height: u64,
    ) {
        let storage = env.storage().persistent();
        storage.set(&DataKey::ClientState(client_id.clone()), &client_state);
        storage.set(
            &DataKey::ConsensusState(client_id.clone(), height),
            &consensus_state,
        );
        storage.set(&DataKey::LatestHeight(client_id), &height);
    }

    pub fn latest_height(env: Env, client_id: String) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestHeight(client_id))
            .unwrap_or(0)
    }

    pub fn client_state(env: Env, client_id: String) -> Bytes {
        env.storage()
            .persistent()
            .get(&DataKey::ClientState(client_id))
            .unwrap_or_else(|| Bytes::new(&env))
    }

    pub fn consensus_state(env: Env, client_id: String, height: u64) -> Bytes {
        env.storage()
            .persistent()
            .get(&DataKey::ConsensusState(client_id, height))
            .unwrap_or_else(|| Bytes::new(&env))
    }

    pub fn verify_client_message(
        _env: Env,
        _client_id: String,
        _client_message: Bytes,
    ) -> (Bytes, u64) {
        (Bytes::new(&_env), 0)
    }

    pub fn check_for_misbehaviour(_env: Env, _client_id: String, _client_message: Bytes) -> bool {
        false
    }

    pub fn update_state(env: Env, client_id: String, _client_message: Bytes) -> u64 {
        let storage = env.storage().persistent();
        let next = storage
            .get::<DataKey, u64>(&DataKey::LatestHeight(client_id.clone()))
            .unwrap_or(0)
            + 1;
        storage.set(&DataKey::LatestHeight(client_id.clone()), &next);
        storage.set(&DataKey::ConsensusState(client_id, next), &Bytes::new(&env));
        next
    }

    pub fn update_state_on_misbehaviour(env: Env, client_id: String, _client_message: Bytes) {
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
        _env: Env,
        _client_id: String,
        _height: u64,
        _proof: Bytes,
        _path: Bytes,
        _value: Bytes,
    ) -> bool {
        true
    }

    pub fn verify_non_membership(
        _env: Env,
        _client_id: String,
        _height: u64,
        _proof: Bytes,
        _path: Bytes,
    ) -> bool {
        true
    }

    pub fn get_timestamp_at_height(_env: Env, _client_id: String, _height: u64) -> u64 {
        0
    }

    pub fn client_type(env: Env) -> Symbol {
        Symbol::new(&env, "mock")
    }
}

mod test;
