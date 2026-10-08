use soroban_sdk::{contractimpl, panic_with_error, Env, String};
use stellar_macros::only_owner;

use crate::{
    errors::Error, storage::DataKey, IbcTransferApp, IbcTransferAppArgs, IbcTransferAppClient,
};

const SECS_PER_DAY: u64 = 86_400;

fn today(env: &Env) -> u64 {
    env.ledger().timestamp() / SECS_PER_DAY
}

pub trait Limits {
    fn set_rate_limit(env: Env, denom: String, daily_cap: i128);

    fn daily_cap(env: Env, denom: String) -> Option<i128>;

    fn daily_usage(env: Env, denom: String) -> i128;
}

#[contractimpl]
impl Limits for IbcTransferApp {
    #[only_owner]
    fn set_rate_limit(env: Env, denom: String, daily_cap: i128) {
        if daily_cap < 0 {
            panic_with_error!(&env, Error::AmountMustBePositive);
        }

        env.storage()
            .persistent()
            .set(&DataKey::DailyCap(denom), &daily_cap);
    }

    fn daily_cap(env: Env, denom: String) -> Option<i128> {
        env.storage().persistent().get(&DataKey::DailyCap(denom))
    }

    fn daily_usage(env: Env, denom: String) -> i128 {
        let day = today(&env);

        env.storage()
            .persistent()
            .get(&DataKey::Usage(denom, day))
            .unwrap_or(0)
    }
}

pub(crate) fn enforce_rate_limit(env: &Env, denom: &String, amount: i128) {
    let cap: Option<i128> = env
        .storage()
        .persistent()
        .get(&DataKey::DailyCap(denom.clone()));
    let cap = match cap {
        Some(c) => c,
        None => return,
    };

    let day = today(env);
    let usage_key = DataKey::Usage(denom.clone(), day);
    let used: i128 = env.storage().persistent().get(&usage_key).unwrap_or(0);

    if used + amount > cap {
        panic_with_error!(env, Error::DailyLimitExceeded);
    }

    env.storage().persistent().set(&usage_key, &(used + amount));
}
