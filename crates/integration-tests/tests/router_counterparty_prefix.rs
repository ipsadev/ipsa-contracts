use integration_tests::router::setup;
use integration_tests::tendermint::{client_state, consensus_state};
use soroban_sdk::{testutils::Events, vec, Address, Bytes, Env, Event, String, Vec};
use stellar_ibc_router::{
    Counterparty, CounterpartyPrefixMigrated, DataKey, Error, IbcRouterClient,
};

fn client(env: &Env, router: &IbcRouterClient<'_>) -> String {
    router.create_client(
        &String::from_str(env, "test"),
        &client_state(env, 1),
        &consensus_state(env, [0u8; 32], 1),
        &1,
    )
}

fn store_counterparty(env: &Env, router_id: &Address, client_id: &String, prefix: Vec<Bytes>) {
    env.as_contract(router_id, || {
        env.storage().persistent().set(
            &DataKey::Counterparty(client_id.clone()),
            &Counterparty {
                client_id: String::from_str(env, "08-wasm-0"),
                commitment_prefix: prefix,
            },
        );
    });
}

#[test]
fn a_prefix_must_have_one_segment_per_proof_level() {
    let (env, _router_id, router, _lc_id) = setup();
    let id = client(&env, &router);
    let counterparty = String::from_str(&env, "08-wasm-0");

    for prefix in [
        Vec::new(&env),
        vec![&env, Bytes::from_slice(&env, b"ibc")],
        vec![
            &env,
            Bytes::from_slice(&env, b"ibc"),
            Bytes::new(&env),
            Bytes::new(&env),
        ],
    ] {
        assert_eq!(
            router.try_register_counterparty(&id, &counterparty, &prefix),
            Err(Ok(Error::InvalidCommitmentPrefix.into()))
        );
    }

    assert!(router.counterparty(&id).is_none());
}

#[test]
fn a_single_segment_prefix_migrates_to_the_ibc_go_spelling() {
    let (env, router_id, router, _lc_id) = setup();
    let id = client(&env, &router);
    store_counterparty(
        &env,
        &router_id,
        &id,
        vec![&env, Bytes::from_slice(&env, b"ibc")],
    );

    router.migrate_counterparty_prefix(&id);

    let migrated = vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)];

    assert_eq!(
        env.events().all().filter_by_contract(&router_id),
        [CounterpartyPrefixMigrated {
            client_id: id.clone(),
            commitment_prefix: migrated.clone(),
        }
        .to_xdr(&env, &router_id)]
    );

    let counterparty = router.counterparty(&id).unwrap();
    assert_eq!(counterparty.commitment_prefix, migrated);
    assert_eq!(counterparty.client_id, String::from_str(&env, "08-wasm-0"));
}

#[test]
fn a_migrated_prefix_cannot_be_migrated_again() {
    let (env, router_id, router, _lc_id) = setup();
    let id = client(&env, &router);
    store_counterparty(
        &env,
        &router_id,
        &id,
        vec![&env, Bytes::from_slice(&env, b"ibc")],
    );

    router.migrate_counterparty_prefix(&id);

    assert_eq!(
        router.try_migrate_counterparty_prefix(&id),
        Err(Ok(Error::PrefixNotMigratable.into()))
    );
}

#[test]
fn an_empty_prefix_is_not_migrated() {
    let (env, router_id, router, _lc_id) = setup();
    let id = client(&env, &router);
    store_counterparty(&env, &router_id, &id, Vec::new(&env));

    assert_eq!(
        router.try_migrate_counterparty_prefix(&id),
        Err(Ok(Error::PrefixNotMigratable.into()))
    );
}

#[test]
fn a_client_without_a_counterparty_has_nothing_to_migrate() {
    let (env, _router_id, router, _lc_id) = setup();
    let id = client(&env, &router);

    assert_eq!(
        router.try_migrate_counterparty_prefix(&id),
        Err(Ok(Error::CounterpartyNotRegistered.into()))
    );
}

#[test]
fn migrating_a_prefix_requires_the_owner() {
    let (env, router_id, router, _lc_id) = setup();
    let id = client(&env, &router);
    store_counterparty(
        &env,
        &router_id,
        &id,
        vec![&env, Bytes::from_slice(&env, b"ibc")],
    );

    env.set_auths(&[]);

    assert!(router.try_migrate_counterparty_prefix(&id).is_err());
    assert_eq!(
        router.counterparty(&id).unwrap().commitment_prefix,
        vec![&env, Bytes::from_slice(&env, b"ibc")]
    );
}
