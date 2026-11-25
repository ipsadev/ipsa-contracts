use integration_tests::router::setup;
use integration_tests::tendermint::{client_state, consensus_state};
use soroban_sdk::{vec, Bytes, String};
use stellar_ibc_router::Error;
use stellar_tendermint_light_client::{TendermintLightClient, TendermintLightClientClient};

fn created(env: &soroban_sdk::Env, router: &stellar_ibc_router::IbcRouterClient) -> String {
    let client_id = router.create_client(
        &String::from_str(env, "test"),
        &client_state(env, 1),
        &consensus_state(env, [0u8; 32], 1),
        &1,
    );

    router.register_counterparty(
        &client_id,
        &String::from_str(env, "07-tendermint-0"),
        &vec![env, Bytes::from_slice(env, b"ibc"), Bytes::new(env)],
    );

    client_id
}

#[test]
fn migration_repoints_the_client_and_seeds_the_replacement() {
    let (env, router_id, router, original) = setup();
    let client_id = created(&env, &router);
    let replacement = env.register(TendermintLightClient, (router_id.clone(),));

    router.migrate_client(
        &client_id,
        &replacement,
        &client_state(&env, 40),
        &consensus_state(&env, [0u8; 32], 1),
        &40,
    );

    assert_eq!(
        router.light_client_address(&client_id),
        Some(replacement.clone())
    );

    assert_eq!(
        TendermintLightClientClient::new(&env, &replacement).latest_height(&client_id),
        40,
        "the replacement was seeded at the migration height"
    );
    assert_eq!(
        TendermintLightClientClient::new(&env, &original).latest_height(&client_id),
        1,
        "the original keeps the state it had before the migration"
    );
}

#[test]
fn migration_keeps_the_counterparty() {
    let (env, router_id, router, _original) = setup();
    let client_id = created(&env, &router);
    let before = router.counterparty(&client_id).expect("registered");
    let replacement = env.register(TendermintLightClient, (router_id.clone(),));

    router.migrate_client(
        &client_id,
        &replacement,
        &client_state(&env, 1),
        &consensus_state(&env, [0u8; 32], 1),
        &2,
    );

    let after = router.counterparty(&client_id).expect("still registered");

    assert_eq!(after.client_id, before.client_id);
    assert_eq!(after.commitment_prefix, before.commitment_prefix);
}

#[test]
fn a_migration_that_leaves_the_client_uninitialised_is_refused() {
    let (env, router_id, router, _original) = setup();
    let client_id = created(&env, &router);
    let replacement = env.register(TendermintLightClient, (router_id.clone(),));

    let result = router.try_migrate_client(
        &client_id,
        &replacement,
        &client_state(&env, 0),
        &consensus_state(&env, [0u8; 32], 1),
        &0,
    );

    assert_eq!(result, Err(Ok(Error::LightClientNotInitialised.into())));
}

#[test]
fn an_unknown_client_cannot_be_migrated() {
    let (env, _router_id, router, original) = setup();

    let result = router.try_migrate_client(
        &String::from_str(&env, "test-9"),
        &original,
        &client_state(&env, 1),
        &consensus_state(&env, [0u8; 32], 1),
        &2,
    );

    assert_eq!(result, Err(Ok(Error::ClientIdNotFound.into())));
}
