mod common;

use common::setup;
use soroban_sdk::{vec, Bytes, String};
use stellar_ibc_router::Error;
use stellar_mock_light_client::{MockLightClient, MockLightClientClient};

fn created(env: &soroban_sdk::Env, router: &stellar_ibc_router::IbcRouterClient) -> String {
    let client_id = router.create_client(
        &String::from_str(env, "mock"),
        &Bytes::from_slice(env, b"client-state"),
        &Bytes::from_slice(env, b"consensus-state"),
        &1,
    );

    router.register_counterparty(
        &client_id,
        &String::from_str(env, "07-tendermint-0"),
        &vec![env, Bytes::from_slice(env, b"ibc")],
    );

    client_id
}

#[test]
fn migration_repoints_the_client_and_later_updates_reach_the_new_contract() {
    let (env, _router_id, router, original) = setup();
    let client_id = created(&env, &router);
    let replacement = env.register(MockLightClient, ());

    router.migrate_client(
        &client_id,
        &replacement,
        &Bytes::from_slice(&env, b"seeded-client-state"),
        &Bytes::from_slice(&env, b"seeded-consensus-state"),
        &40,
    );

    assert_eq!(
        router.light_client_address(&client_id),
        Some(replacement.clone())
    );

    let reached = router.update_client(&client_id, &Bytes::from_slice(&env, b"header"));

    assert_eq!(reached, 41);
    assert_eq!(
        MockLightClientClient::new(&env, &original).latest_height(&client_id),
        1
    );
}

#[test]
fn migration_keeps_the_counterparty() {
    let (env, _router_id, router, _original) = setup();
    let client_id = created(&env, &router);
    let before = router.counterparty(&client_id).expect("registered");
    let replacement = env.register(MockLightClient, ());

    router.migrate_client(
        &client_id,
        &replacement,
        &Bytes::from_slice(&env, b"client-state"),
        &Bytes::from_slice(&env, b"consensus-state"),
        &2,
    );

    let after = router.counterparty(&client_id).expect("still registered");

    assert_eq!(after.client_id, before.client_id);
    assert_eq!(after.commitment_prefix, before.commitment_prefix);
}

#[test]
fn a_migration_that_leaves_the_client_uninitialised_is_refused() {
    let (env, _router_id, router, _original) = setup();
    let client_id = created(&env, &router);
    let replacement = env.register(MockLightClient, ());

    let result = router.try_migrate_client(
        &client_id,
        &replacement,
        &Bytes::from_slice(&env, b"client-state"),
        &Bytes::from_slice(&env, b"consensus-state"),
        &0,
    );

    assert_eq!(result, Err(Ok(Error::LightClientNotInitialised.into())));
}

#[test]
fn an_unknown_client_cannot_be_migrated() {
    let (env, _router_id, router, original) = setup();

    let result = router.try_migrate_client(
        &String::from_str(&env, "mock-9"),
        &original,
        &Bytes::from_slice(&env, b"client-state"),
        &Bytes::from_slice(&env, b"consensus-state"),
        &2,
    );

    assert_eq!(result, Err(Ok(Error::ClientIdNotFound.into())));
}
