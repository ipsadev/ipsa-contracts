use soroban_sdk::{contracttype, panic_with_error, Address, BytesN, Env, String};

use crate::{
    errors::Error,
    types::{Config, Fill},
};

const DAY_IN_LEDGERS: u32 = 17_280;

const TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;

const TTL_EXTEND_TO: u32 = 30 * DAY_IN_LEDGERS;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
    Route(String, String),
    Denom(String, Address),
    Fill(BytesN<32>),
    Settled(BytesN<32>),
    Notified(BytesN<32>),
}

pub fn config(env: &Env) -> Config {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotConfigured))
}

pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&DataKey::Config, config);
}

pub fn route(env: &Env, dest_client: &String, denom: &String) -> Option<Address> {
    read(env, &DataKey::Route(dest_client.clone(), denom.clone()))
}

pub fn denom(env: &Env, dest_client: &String, token: &Address) -> Option<String> {
    read(env, &DataKey::Denom(dest_client.clone(), token.clone()))
}

pub fn set_route(env: &Env, dest_client: &String, denom: &String, token: &Address) {
    write(
        env,
        &DataKey::Route(dest_client.clone(), denom.clone()),
        token,
    );
    write(
        env,
        &DataKey::Denom(dest_client.clone(), token.clone()),
        denom,
    );
}

pub fn remove_route(env: &Env, dest_client: &String, denom: &String) {
    if let Some(token) = route(env, dest_client, denom) {
        env.storage()
            .persistent()
            .remove(&DataKey::Denom(dest_client.clone(), token));
    }

    env.storage()
        .persistent()
        .remove(&DataKey::Route(dest_client.clone(), denom.clone()));
}

pub fn fill(env: &Env, key: &BytesN<32>) -> Option<Fill> {
    read(env, &DataKey::Fill(key.clone()))
}

pub fn set_fill(env: &Env, key: &BytesN<32>, fill: &Fill) {
    write(env, &DataKey::Fill(key.clone()), fill);
}

pub fn settled(env: &Env, key: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Settled(key.clone()))
}

pub fn set_settled(env: &Env, key: &BytesN<32>) {
    write(env, &DataKey::Settled(key.clone()), &true);
}

pub fn notified(env: &Env, key: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Notified(key.clone()))
}

pub fn set_notified(env: &Env, key: &BytesN<32>) {
    write(env, &DataKey::Notified(key.clone()), &true);
}

pub fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn read<V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(env: &Env, key: &DataKey) -> Option<V> {
    let value = env.storage().persistent().get(key);

    if value.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    value
}

fn write<V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &DataKey, value: &V) {
    env.storage().persistent().set(key, value);
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}
