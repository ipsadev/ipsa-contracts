use integration_tests::router::setup;
use integration_tests::tendermint::{client_state, consensus_state};
use soroban_sdk::{testutils::Address as _, vec, Bytes, String};
use stellar_ibc_router::Error;
use stellar_tendermint_light_client::TendermintLightClientClient;

#[test]
fn register_client_type_stores_light_client_address() {
    let (env, _router_id, router, light_client_id) = setup();
    let resolved = router
        .client_type_address(&String::from_str(&env, "test"))
        .unwrap();
    assert_eq!(resolved, light_client_id);
}

#[test]
fn create_client_initialises_lc_and_returns_unique_id() {
    let (env, _router_id, router, light_client_id) = setup();

    let client_state_bytes = client_state(&env, 1);
    let consensus_state_bytes = consensus_state(&env, [0u8; 32], 1);
    let id1 = router.create_client(
        &String::from_str(&env, "test"),
        &client_state_bytes,
        &consensus_state_bytes,
        &1,
    );
    let id2 = router.create_client(
        &String::from_str(&env, "test"),
        &client_state_bytes,
        &consensus_state_bytes,
        &1,
    );

    assert_ne!(id1, id2);
    assert_eq!(id1, String::from_str(&env, "test-0"));
    assert_eq!(id2, String::from_str(&env, "test-1"));

    let lc = TendermintLightClientClient::new(&env, &light_client_id);
    assert_eq!(lc.latest_height(&id1), 1);
    assert_eq!(lc.client_state(&id1), client_state_bytes);
}

#[test]
fn register_counterparty_stores_mapping() {
    let (env, _router_id, router, _lc_id) = setup();

    let id = router.create_client(
        &String::from_str(&env, "test"),
        &client_state(&env, 1),
        &consensus_state(&env, [0u8; 32], 1),
        &1,
    );

    let counterparty_id = String::from_str(&env, "07-tendermint-0");
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)];
    router.register_counterparty(&id, &counterparty_id, &prefix);

    let cp = router.counterparty(&id).unwrap();
    assert_eq!(cp.client_id, counterparty_id);
    assert_eq!(cp.commitment_prefix.len(), 2);
}

#[test]
fn register_counterparty_rejects_duplicate() {
    let (env, _router_id, router, _lc_id) = setup();
    let id = router.create_client(
        &String::from_str(&env, "test"),
        &client_state(&env, 1),
        &consensus_state(&env, [0u8; 32], 1),
        &1,
    );
    let cp_id = String::from_str(&env, "07-tendermint-0");
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)];
    router.register_counterparty(&id, &cp_id, &prefix);

    let result = router.try_register_counterparty(&id, &cp_id, &prefix);
    assert_eq!(result, Err(Ok(Error::CounterpartyAlreadyRegistered.into())));
}

#[test]
fn register_counterparty_rejects_unknown_client() {
    let (env, _router_id, router, _lc_id) = setup();
    let result = router.try_register_counterparty(
        &String::from_str(&env, "test-999"),
        &String::from_str(&env, "07-tendermint-0"),
        &vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)],
    );
    assert_eq!(result, Err(Ok(Error::ClientIdNotFound.into())));
}

#[test]
#[should_panic]
fn update_client_refuses_a_header_the_light_client_cannot_verify() {
    let (env, _router_id, router, _lc_id) = setup();
    let id = router.create_client(
        &String::from_str(&env, "test"),
        &client_state(&env, 1),
        &consensus_state(&env, [0u8; 32], 1),
        &5,
    );

    router.update_client(&id, &Bytes::from_slice(&env, b"msg"));
}

#[test]
fn register_counterparty_requires_authorisation() {
    let (env, _router_id, router, _lc_id) = setup();
    let id = router.create_client(
        &String::from_str(&env, "test"),
        &client_state(&env, 1),
        &consensus_state(&env, [0u8; 32], 1),
        &1,
    );

    env.set_auths(&[]);

    let result = router.try_register_counterparty(
        &id,
        &String::from_str(&env, "07-tendermint-0"),
        &vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)],
    );

    assert!(
        result.is_err(),
        "an unauthorised caller must not be able to bind a client to a counterparty"
    );
}

#[test]
fn register_port_requires_authorisation() {
    let (env, _router_id, router, _lc_id) = setup();
    let app = soroban_sdk::Address::generate(&env);

    env.set_auths(&[]);

    assert!(
        router
            .try_register_port(&String::from_str(&env, "transfer"), &app)
            .is_err(),
        "port registration is owner-gated and must reject an unauthorised caller"
    );
}
