#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, vec, Address, Bytes,
    Env, IntoVal, String, Symbol,
};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    UntrustedGmpApp = 1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmpAcknowledged {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
    pub success: bool,
    pub result: Bytes,
    pub acknowledgement: Bytes,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmpTimedOut {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Pending,
    Acknowledged(GmpAcknowledged),
    TimedOut(GmpTimedOut),
}

#[contracttype]
enum DataKey {
    Gmp,
    Outcome(String, u64),
}

#[contract]
pub struct IpsaGmpCaller;

#[contractimpl(contracttrait)]
impl Ownable for IpsaGmpCaller {}

#[contractimpl]
impl IpsaGmpCaller {
    pub fn __constructor(env: Env, owner: Address, gmp: Address) {
        set_owner(&env, &owner);
        env.storage().instance().set(&DataKey::Gmp, &gmp);
    }

    #[only_owner]
    pub fn send_call(
        env: Env,
        source_client_id: String,
        receiver: String,
        salt: Bytes,
        payload: Bytes,
        timeout_timestamp: u64,
        memo: String,
        encoding: String,
    ) -> u64 {
        let sequence: u64 = env.invoke_contract(
            &gmp(&env),
            &Symbol::new(&env, "send_call"),
            vec![
                &env,
                env.current_contract_address().into_val(&env),
                source_client_id.into_val(&env),
                receiver.into_val(&env),
                salt.into_val(&env),
                payload.into_val(&env),
                timeout_timestamp.into_val(&env),
                memo.into_val(&env),
                encoding.into_val(&env),
            ],
        );

        record(&env, &source_client_id, sequence, &Outcome::Pending);

        sequence
    }

    pub fn on_gmp_acknowledgement(env: Env, gmp_app: Address, result: GmpAcknowledged) {
        trusted(&env, &gmp_app);

        record(
            &env,
            &result.source_client.clone(),
            result.sequence,
            &Outcome::Acknowledged(result),
        );
    }

    pub fn on_gmp_timeout(env: Env, gmp_app: Address, result: GmpTimedOut) {
        trusted(&env, &gmp_app);

        record(
            &env,
            &result.source_client.clone(),
            result.sequence,
            &Outcome::TimedOut(result),
        );
    }

    pub fn outcome(env: Env, source_client_id: String, sequence: u64) -> Option<Outcome> {
        env.storage()
            .persistent()
            .get(&DataKey::Outcome(source_client_id, sequence))
    }

    pub fn gmp(env: Env) -> Address {
        gmp(&env)
    }
}

fn gmp(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Gmp).unwrap()
}

fn trusted(env: &Env, gmp_app: &Address) {
    if *gmp_app != gmp(env) {
        panic_with_error!(env, Error::UntrustedGmpApp);
    }

    gmp_app.require_auth();
}

fn record(env: &Env, source_client_id: &String, sequence: u64, outcome: &Outcome) {
    env.storage().persistent().set(
        &DataKey::Outcome(source_client_id.clone(), sequence),
        outcome,
    );
}
