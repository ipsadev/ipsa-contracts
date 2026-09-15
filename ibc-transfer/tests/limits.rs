mod common;

use common::{fund, register_local, setup, xlm};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, String};
use stellar_ibc_transfer::Error;

#[test]
fn set_rate_limit_stores_cap_and_starts_at_zero_usage() {
    let f = setup();
    f.transfer.set_rate_limit(&xlm(&f.env), &500);
    assert_eq!(f.transfer.daily_cap(&xlm(&f.env)), Some(500));
    assert_eq!(f.transfer.daily_usage(&xlm(&f.env)), 0);
}

#[test]
fn initiate_transfer_under_cap_accumulates_usage() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 1_000);
    f.transfer.set_rate_limit(&xlm(&f.env), &500);

    f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &200,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &150,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(f.transfer.daily_usage(&xlm(&f.env)), 350);
}

#[test]
fn initiate_transfer_over_cap_is_rejected() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 10_000);
    f.transfer.set_rate_limit(&xlm(&f.env), &500);

    f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &400,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    let result = f.transfer.try_initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &200,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(result, Err(Ok(Error::DailyLimitExceeded.into())));
    assert_eq!(f.transfer.daily_usage(&xlm(&f.env)), 400);
}

#[test]
fn rate_limit_resets_after_day_rolls_over() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 10_000);
    f.transfer.set_rate_limit(&xlm(&f.env), &500);

    f.env.ledger().set_timestamp(86_400);
    f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &500,
        &String::from_str(&f.env, "cosmos1abc"),
        &172_800,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(f.transfer.daily_usage(&xlm(&f.env)), 500);

    f.env.ledger().set_timestamp(172_800);
    assert_eq!(f.transfer.daily_usage(&xlm(&f.env)), 0);

    f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &500,
        &String::from_str(&f.env, "cosmos1abc"),
        &200_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(f.transfer.daily_usage(&xlm(&f.env)), 500);
}

#[test]
fn rate_limit_unset_denom_is_unlimited() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 10_000);
    f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &9_999,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(f.transfer.daily_usage(&xlm(&f.env)), 0);
    assert_eq!(f.transfer.daily_cap(&xlm(&f.env)), None);
}
