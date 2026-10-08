mod common;

use common::setup;
use soroban_sdk::{testutils::Address as _, Address, String};
use stellar_ibc_router::Error;

#[test]
fn register_port_stores_app_address() {
    let (env, _router_id, router, _lc_id) = setup();
    let app = Address::generate(&env);
    let port_id = String::from_str(&env, "transfer");

    assert!(router.port_app(&port_id).is_none());
    router.register_port(&port_id, &app);
    assert_eq!(router.port_app(&port_id).unwrap(), app);
}

#[test]
fn register_port_distinct_ports_isolated() {
    let (env, _router_id, router, _lc_id) = setup();
    let app_a = Address::generate(&env);
    let app_b = Address::generate(&env);
    let port_a = String::from_str(&env, "transfer");
    let port_b = String::from_str(&env, "echo");

    router.register_port(&port_a, &app_a);
    router.register_port(&port_b, &app_b);

    assert_eq!(router.port_app(&port_a).unwrap(), app_a);
    assert_eq!(router.port_app(&port_b).unwrap(), app_b);
}

#[test]
fn register_port_rejects_duplicate() {
    let (env, _router_id, router, _lc_id) = setup();
    let app_a = Address::generate(&env);
    let app_b = Address::generate(&env);
    let port_id = String::from_str(&env, "transfer");

    router.register_port(&port_id, &app_a);
    let result = router.try_register_port(&port_id, &app_b);
    assert_eq!(result, Err(Ok(Error::PortAlreadyRegistered.into())));
}
