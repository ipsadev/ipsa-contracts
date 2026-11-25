use integration_tests::router::{
    flow_committing_in, make_payload, sent_packet_in, setup, setup_packet_flow, timeout_flow,
};
use integration_tests::tendermint::{Entry, HEIGHT};
use soroban_sdk::{testutils::Address as _, vec, Address, Bytes, String};
use stellar_ibc_router::Error;

#[test]
fn sending_counts_a_packet_in_flight_and_acknowledging_retires_it() {
    let env = soroban_sdk::Env::default();
    let outbound = sent_packet_in(&env, 1, 2_000, b"hello");
    let acks = vec![&env, Bytes::from_slice(&env, b"ack")];
    let f = flow_committing_in(env, &[Entry::Acknowledgement(outbound.clone(), acks)], 1);
    let payload = make_payload(&f.env, &f.port_id, b"hello");

    f.router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    assert_eq!(f.router.port_packets_in_flight(&f.port_id), 1);

    f.router.acknowledge_packet(
        &outbound,
        &vec![&f.env, Bytes::from_slice(&f.env, b"ack")],
        &f.proofs[0].clone(),
        &HEIGHT,
    );

    assert_eq!(f.router.port_packets_in_flight(&f.port_id), 0);
}

#[test]
fn remove_port_unbinds_an_idle_port() {
    let (env, _router_id, router, _light_client) = setup();
    let app = Address::generate(&env);
    let port_id = String::from_str(&env, "custom-app");

    router.register_port(&port_id, &app);
    router.remove_port(&port_id);

    assert!(router.port_app(&port_id).is_none());
}

#[test]
fn a_removed_port_can_be_registered_again() {
    let (env, _router_id, router, _light_client) = setup();
    let first_app = Address::generate(&env);
    let second_app = Address::generate(&env);
    let port_id = String::from_str(&env, "custom-app");

    router.register_port(&port_id, &first_app);
    router.remove_port(&port_id);
    router.register_port(&port_id, &second_app);

    assert_eq!(router.port_app(&port_id), Some(second_app));
}

#[test]
fn remove_port_is_refused_while_packets_are_in_flight() {
    let f = setup_packet_flow();
    let payload = make_payload(&f.env, &f.port_id, b"hello");

    f.router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    let result = f.router.try_remove_port(&f.port_id);

    assert_eq!(result, Err(Ok(Error::PortHasPacketsInFlight.into())));
    assert_eq!(f.router.port_app(&f.port_id), Some(f.app.clone()));
}

#[test]
fn remove_port_succeeds_once_the_last_packet_times_out() {
    let (f, packet) = timeout_flow(1, b"hello");
    let payload = make_payload(&f.env, &f.port_id, b"hello");

    f.router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    f.router
        .timeout_packet(&packet, &f.proofs[0].clone(), &HEIGHT);
    f.router.remove_port(&f.port_id);

    assert!(f.router.port_app(&f.port_id).is_none());
}

#[test]
fn remove_port_rejects_an_unregistered_port() {
    let (env, _router_id, router, _light_client) = setup();

    let result = router.try_remove_port(&String::from_str(&env, "custom-app"));

    assert_eq!(result, Err(Ok(Error::PortNotRegistered.into())));
}

#[test]
fn reassign_port_validates_the_identifier() {
    let (env, _router_id, router, _light_client) = setup();
    let app = Address::generate(&env);

    let result = router.try_reassign_port(&String::from_str(&env, "abc"), &app);

    assert_eq!(result, Err(Ok(Error::InvalidIdentifierLength.into())));
}

#[test]
fn reassign_and_remove_port_require_the_owner() {
    let (env, _router_id, router, _light_client) = setup();
    let app = Address::generate(&env);
    let other_app = Address::generate(&env);
    let port_id = String::from_str(&env, "custom-app");

    router.register_port(&port_id, &app);
    env.set_auths(&[]);

    assert!(router.try_reassign_port(&port_id, &other_app).is_err());
    assert!(router.try_remove_port(&port_id).is_err());
    assert_eq!(router.port_app(&port_id), Some(app));
}
