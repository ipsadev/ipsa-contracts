use soroban_sdk::{panic_with_error, Address, Bytes, BytesN, Env, IntoVal, String, Val, Vec};

use crate::{
    errors::Error,
    storage::{self, account_wasm},
    types::DerivedAccount,
};

pub fn salt_for(env: &Env, client_id: &String, sender: &String, salt: &Bytes) -> BytesN<32> {
    ipsa::gmp::account_salt(env, client_id, sender, salt)
}

pub fn address_of(env: &Env, client_id: &String, sender: &String, salt: &Bytes) -> Address {
    env.deployer()
        .with_current_contract(salt_for(env, client_id, sender, salt))
        .deployed_address()
}

pub fn derived(env: &Env, client_id: &String, sender: &String, salt: &Bytes) -> DerivedAccount {
    DerivedAccount {
        address: address_of(env, client_id, sender, salt),
        materialized: storage::is_materialized(env, &salt_for(env, client_id, sender, salt)),
    }
}

pub fn ensure(env: &Env, client_id: &String, sender: &String, salt: &Bytes) -> Address {
    let derivation = salt_for(env, client_id, sender, salt);

    if storage::is_materialized(env, &derivation) {
        return address_of(env, client_id, sender, salt);
    }

    let wasm =
        account_wasm(env).unwrap_or_else(|| panic_with_error!(env, Error::AccountWasmNotSet));

    let controller: Val = env.current_contract_address().into_val(env);
    let args: Vec<Val> = soroban_sdk::vec![env, controller];

    let address = env
        .deployer()
        .with_current_contract(derivation.clone())
        .deploy_v2(wasm, args);

    storage::mark_materialized(env, &derivation);

    address
}
