mod common;

use common::{recv_packet_for, setup_packet_flow, PacketFlow};
use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Address, Bytes, Env, String, Symbol, Vec,
};
use stellar_ibc_router::{ics24, Counterparty, DataKey, Error};

const LAST_PATH: Symbol = symbol_short!("LASTPATH");

#[contract]
pub struct OneLevelLightClient;

#[contractimpl]
impl OneLevelLightClient {
    pub fn initialise(
        _env: Env,
        _client_id: String,
        _client_state: Bytes,
        _consensus_state: Bytes,
        _height: u64,
    ) {
    }

    pub fn proof_levels(_env: Env) -> u32 {
        1
    }

    pub fn verify_membership(
        env: Env,
        _client_id: String,
        _height: u64,
        _proof: Bytes,
        path: Vec<Bytes>,
        _value: Bytes,
    ) -> bool {
        env.storage().instance().set(&LAST_PATH, &path);

        true
    }

    pub fn last_path(env: Env) -> Vec<Bytes> {
        env.storage().instance().get(&LAST_PATH).unwrap()
    }
}

fn one_level_client(f: &PacketFlow) -> (String, Address) {
    let client_type = String::from_str(&f.env, "sepolia");
    let light_client = f.env.register(OneLevelLightClient, ());

    f.router.register_client_type(&client_type, &light_client);

    let client_id = f.router.create_client(
        &client_type,
        &Bytes::from_slice(&f.env, b"cs"),
        &Bytes::from_slice(&f.env, b"cons"),
        &1,
    );

    (client_id, light_client)
}

#[test]
fn an_empty_prefix_gives_a_one_level_client_the_bare_key() {
    let mut f = setup_packet_flow();
    let (client_id, light_client) = one_level_client(&f);

    f.router.register_counterparty(
        &client_id,
        &f.counterparty_client_id,
        &vec![&f.env, Bytes::new(&f.env)],
    );
    f.client_id = client_id;

    let packet = recv_packet_for(&f, 1, b"hello");
    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    let key = ics24::packet_commitment_path(&f.env, &f.counterparty_client_id, 1);

    assert_eq!(
        OneLevelLightClientClient::new(&f.env, &light_client).last_path(),
        vec![&f.env, key]
    );
}

#[test]
fn a_two_segment_prefix_is_refused_for_a_one_level_client() {
    let f = setup_packet_flow();
    let (client_id, _) = one_level_client(&f);

    assert_eq!(
        f.router.try_register_counterparty(
            &client_id,
            &f.counterparty_client_id,
            &vec![
                &f.env,
                Bytes::from_slice(&f.env, b"ibc"),
                Bytes::new(&f.env)
            ],
        ),
        Err(Ok(Error::InvalidCommitmentPrefix.into()))
    );
}

#[test]
fn the_prefix_migration_is_refused_for_a_one_level_client() {
    let f = setup_packet_flow();
    let (client_id, _) = one_level_client(&f);

    f.router.register_counterparty(
        &client_id,
        &f.counterparty_client_id,
        &vec![&f.env, Bytes::new(&f.env)],
    );

    assert_eq!(
        f.router.try_migrate_counterparty_prefix(&client_id),
        Err(Ok(Error::InvalidCommitmentPrefix.into()))
    );

    let stored: Counterparty = f.env.as_contract(&f.router_id, || {
        f.env
            .storage()
            .persistent()
            .get(&DataKey::Counterparty(client_id.clone()))
            .unwrap()
    });

    assert_eq!(stored.commitment_prefix, vec![&f.env, Bytes::new(&f.env)]);
}
