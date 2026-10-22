mod common;

use common::setup;
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Bytes, Event, String,
};
use stellar_ibc_router::{
    ClientCreated, ClientTypeRegistered, ClientUpdated, CounterpartyRegistered, PortReassigned,
    PortRegistered, PortRemoved,
};

#[test]
fn client_lifecycle_publishes_events() {
    let (env, router_id, router, light_client) = setup();
    let client_type = String::from_str(&env, "test");
    let client_id = router.create_client(
        &client_type,
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );
    let created = ClientCreated {
        client_id: client_id.clone(),
        client_type: client_type.clone(),
        height: 1,
    };

    assert_eq!(
        env.events().all().filter_by_contract(&router_id),
        [created.to_xdr(&env, &router_id)]
    );

    let counterparty_client_id = String::from_str(&env, "07-tendermint-0");

    router.register_counterparty(
        &client_id,
        &counterparty_client_id,
        &vec![&env, Bytes::from_slice(&env, b"ibc")],
    );

    let registered = CounterpartyRegistered {
        client_id: client_id.clone(),
        counterparty_client_id,
    };

    assert_eq!(
        env.events().all().filter_by_contract(&router_id),
        [registered.to_xdr(&env, &router_id)]
    );

    let height = router.update_client(&client_id, &Bytes::from_slice(&env, b"header"));
    let updated = ClientUpdated { client_id, height };

    assert_eq!(
        env.events().all().filter_by_contract(&router_id),
        [updated.to_xdr(&env, &router_id)]
    );

    router.register_client_type(&client_type, &light_client);

    let type_registered = ClientTypeRegistered {
        client_type,
        light_client,
    };

    assert_eq!(
        env.events().all().filter_by_contract(&router_id),
        [type_registered.to_xdr(&env, &router_id)]
    );
}

#[test]
fn port_lifecycle_publishes_events() {
    let (env, router_id, router, _light_client) = setup();
    let port_id = String::from_str(&env, "custom-app");
    let first_app = Address::generate(&env);
    let second_app = Address::generate(&env);

    router.register_port(&port_id, &first_app);

    let registered = PortRegistered {
        port_id: port_id.clone(),
        app: first_app.clone(),
    };

    assert_eq!(
        env.events().all().filter_by_contract(&router_id),
        [registered.to_xdr(&env, &router_id)]
    );

    router.reassign_port(&port_id, &second_app);

    let reassigned = PortReassigned {
        port_id: port_id.clone(),
        previous_app: first_app,
        app: second_app.clone(),
    };

    assert_eq!(
        env.events().all().filter_by_contract(&router_id),
        [reassigned.to_xdr(&env, &router_id)]
    );

    router.remove_port(&port_id);

    let removed = PortRemoved {
        port_id,
        previous_app: second_app,
    };

    assert_eq!(
        env.events().all().filter_by_contract(&router_id),
        [removed.to_xdr(&env, &router_id)]
    );
}
