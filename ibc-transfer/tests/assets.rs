mod common;

use common::{register_voucher, setup, xlm};
use soroban_sdk::{testutils::Address as _, Address, String};

#[test]
fn initiate_transfer_rejects_unheld_foreign_asset() {
    let f = setup();
    let sender = Address::generate(&f.env);

    // A foreign denom with no factory voucher deployed and no balance held cannot
    // be sent (nothing to burn).
    let result = f.transfer.try_initiate_transfer(
        &sender,
        &f.source_client_id,
        &String::from_str(&f.env, "UNKNOWN"),
        &100,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert!(result.is_err());
}

#[test]
fn mint_voucher_and_balance_of_round_trip() {
    let f = setup();
    register_voucher(&f, &xlm(&f.env));
    let who = Address::generate(&f.env);
    assert_eq!(f.transfer.balance_of(&who, &xlm(&f.env)), 0);
    f.transfer.mint(&who, &xlm(&f.env), &123);
    assert_eq!(f.transfer.balance_of(&who, &xlm(&f.env)), 123);
}

#[test]
fn assets_lists_every_registered_denom() {
    let f = setup();
    register_voucher(&f, &xlm(&f.env));
    register_voucher(&f, &String::from_str(&f.env, "EURC"));

    let listed = f.transfer.assets();
    assert_eq!(listed.len(), 2);

    let denoms: std::vec::Vec<String> = listed.iter().map(|entry| entry.denom.clone()).collect();
    assert!(denoms.contains(&xlm(&f.env)));
    assert!(denoms.contains(&String::from_str(&f.env, "EURC")));
}

#[test]
fn assets_is_empty_before_anything_is_registered() {
    let f = setup();
    assert_eq!(f.transfer.assets().len(), 0);
}

#[test]
fn registering_the_same_denom_twice_does_not_duplicate_it() {
    let f = setup();
    register_voucher(&f, &xlm(&f.env));
    register_voucher(&f, &xlm(&f.env));

    assert_eq!(f.transfer.assets().len(), 1);
}

#[test]
fn unregistering_removes_the_denom_from_the_listing() {
    let f = setup();
    register_voucher(&f, &xlm(&f.env));
    register_voucher(&f, &String::from_str(&f.env, "EURC"));

    f.transfer.unregister_asset(&xlm(&f.env));

    let listed = f.transfer.assets();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed.get(0).unwrap().denom,
        String::from_str(&f.env, "EURC")
    );
}

#[test]
fn a_listed_entry_carries_the_address_it_was_registered_with() {
    let f = setup();
    let sac = register_voucher(&f, &xlm(&f.env));

    let entry = f.transfer.assets().get(0).unwrap();
    assert_eq!(entry.denom, xlm(&f.env));
    assert_eq!(entry.address, sac);
}

#[test]
fn asset_returns_the_registered_asset() {
    let f = common::setup();
    let address = common::register_local(&f, &common::xlm(&f.env));
    let info = f.transfer.asset(&common::xlm(&f.env));

    assert_eq!(info.address, address);
    assert!(info.kind == stellar_ibc_transfer::AssetKind::Local);
}

#[test]
fn asset_rejects_an_unknown_denom() {
    let f = common::setup();

    let result = f.transfer.try_asset(&common::xlm(&f.env));

    assert!(matches!(
        result,
        Err(Ok(error)) if error == stellar_ibc_transfer::Error::UnknownAsset.into()
    ));
}
