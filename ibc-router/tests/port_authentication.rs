mod common;

use common::{mk_payload, setup_packet_flow};
use soroban_sdk::{contract, contractimpl, vec, Address, Env, String};
use stellar_ibc_router::IbcRouterClient;

#[contract]
pub struct SendingApp;

#[contractimpl]
impl SendingApp {
    pub fn send(env: Env, router: Address, client_id: String, port_id: String) -> u64 {
        let payload = mk_payload(&env, &port_id, b"hello");

        IbcRouterClient::new(&env, &router).send_packet(&client_id, &2_000, &vec![&env, payload])
    }
}

#[test]
fn the_application_that_owns_a_port_can_send_on_it() {
    let f = setup_packet_flow();
    let app = f.env.register(SendingApp, ());
    let port_id = String::from_str(&f.env, "sending-app");

    f.router.register_port(&port_id, &app);
    f.env.set_auths(&[]);

    let sequence = SendingAppClient::new(&f.env, &app).send(&f.router_id, &f.client_id, &port_id);

    assert_eq!(sequence, 1);
}

#[test]
fn another_contract_cannot_send_on_a_port_it_does_not_own() {
    let f = setup_packet_flow();
    let owner = f.env.register(SendingApp, ());
    let impostor = f.env.register(SendingApp, ());
    let port_id = String::from_str(&f.env, "sending-app");

    f.router.register_port(&port_id, &owner);
    f.env.set_auths(&[]);

    let result =
        SendingAppClient::new(&f.env, &impostor).try_send(&f.router_id, &f.client_id, &port_id);

    assert!(result.is_err());
    assert!(f.router.packet_commitment(&f.client_id, &1).is_none());
}

#[test]
fn an_account_cannot_send_on_an_application_port_without_its_authorisation() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");

    f.env.set_auths(&[]);

    let result = f
        .router
        .try_send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    assert!(result.is_err());
    assert!(f.router.packet_commitment(&f.client_id, &1).is_none());
}
