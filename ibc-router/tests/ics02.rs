mod common;

use common::setup;
use ipsa_test_light_client::TestLightClientClient;
use soroban_sdk::{testutils::Address as _, vec, Bytes, String};
use stellar_ibc_router::Error;

#[test]
fn register_client_type_stores_light_client_address() {
    let (env, _router_id, router, lc_id) = setup();
    let resolved = router
        .client_type_address(&String::from_str(&env, "test"))
        .unwrap();
    assert_eq!(resolved, lc_id);
}

#[test]
fn create_client_initialises_lc_and_returns_unique_id() {
    let (env, _router_id, router, lc_id) = setup();

    let cs = Bytes::from_slice(&env, b"client-state");
    let cons = Bytes::from_slice(&env, b"consensus-state");
    let id1 = router.create_client(&String::from_str(&env, "test"), &cs, &cons, &1);
    let id2 = router.create_client(&String::from_str(&env, "test"), &cs, &cons, &1);

    assert_ne!(id1, id2);
    assert_eq!(id1, String::from_str(&env, "test-0"));
    assert_eq!(id2, String::from_str(&env, "test-1"));

    let lc = TestLightClientClient::new(&env, &lc_id);
    assert_eq!(lc.latest_height(&id1), 1);
    assert_eq!(lc.client_state(&id1), cs);
}

#[test]
fn register_counterparty_stores_mapping() {
    let (env, _router_id, router, _lc_id) = setup();

    let id = router.create_client(
        &String::from_str(&env, "test"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );

    let counterparty_id = String::from_str(&env, "07-tendermint-0");
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc")];
    router.register_counterparty(&id, &counterparty_id, &prefix);

    let cp = router.counterparty(&id).unwrap();
    assert_eq!(cp.client_id, counterparty_id);
    assert_eq!(cp.commitment_prefix.len(), 1);
}

#[test]
fn register_counterparty_rejects_duplicate() {
    let (env, _router_id, router, _lc_id) = setup();
    let id = router.create_client(
        &String::from_str(&env, "test"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );
    let cp_id = String::from_str(&env, "07-tendermint-0");
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc")];
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
        &vec![&env, Bytes::from_slice(&env, b"ibc")],
    );
    assert_eq!(result, Err(Ok(Error::ClientIdNotFound.into())));
}

#[test]
fn update_client_bumps_height_via_lc() {
    let (env, _router_id, router, lc_id) = setup();
    let id = router.create_client(
        &String::from_str(&env, "test"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &5,
    );

    let new_h = router.update_client(&id, &Bytes::from_slice(&env, b"msg"));
    assert_eq!(new_h, 6);
    assert_eq!(
        TestLightClientClient::new(&env, &lc_id).latest_height(&id),
        6
    );
    assert!(!router.frozen(&id));
}

#[test]
fn register_counterparty_requires_authorisation() {
    let (env, _router_id, router, _lc_id) = setup();
    let id = router.create_client(
        &String::from_str(&env, "test"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );

    env.set_auths(&[]);

    let result = router.try_register_counterparty(
        &id,
        &String::from_str(&env, "07-tendermint-0"),
        &vec![&env, Bytes::from_slice(&env, b"ibc")],
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
