use soroban_sdk::{
    panic_with_error, xdr::ToXdr, Address, Bytes, BytesN, Env, IntoVal, String, Val, Vec,
};

use crate::errors::Error;
use crate::storage::{self, account_wasm};
use crate::types::DerivedAccount;

pub fn salt_for(env: &Env, client_id: &String, sender: &String, salt: &Bytes) -> BytesN<32> {
    let mut preimage = Bytes::new(env);

    append_length_prefixed(env, &mut preimage, &client_id.clone().to_xdr(env));
    append_length_prefixed(env, &mut preimage, &sender.clone().to_xdr(env));
    append_length_prefixed(env, &mut preimage, salt);

    env.crypto().sha256(&preimage).into()
}

fn append_length_prefixed(env: &Env, out: &mut Bytes, part: &Bytes) {
    let len = part.len();

    out.append(&Bytes::from_array(env, &len.to_be_bytes()));
    out.append(part);

    let _ = env;
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
