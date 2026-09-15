use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol};

const ROUTER: Symbol = symbol_short!("router");
const ACCTWASM: Symbol = symbol_short!("acctwasm");

pub fn set_router(env: &Env, router: &Address) {
    env.storage().instance().set(&ROUTER, router);
}

pub fn router(env: &Env) -> Option<Address> {
    env.storage().instance().get(&ROUTER)
}

pub fn set_account_wasm(env: &Env, wasm: &BytesN<32>) {
    env.storage().instance().set(&ACCTWASM, wasm);
}

pub fn account_wasm(env: &Env) -> Option<BytesN<32>> {
    env.storage().instance().get(&ACCTWASM)
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Account(BytesN<32>),
}

pub fn mark_materialized(env: &Env, salt: &BytesN<32>) {
    env.storage()
        .persistent()
        .set(&DataKey::Account(salt.clone()), &true);
}

pub fn is_materialized(env: &Env, salt: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Account(salt.clone()))
        .unwrap_or(false)
}
