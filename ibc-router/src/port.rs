use soroban_sdk::{contractimpl, panic_with_error, Address, Env, String};
use stellar_macros::only_owner;

use crate::{
    errors::Error,
    events::{PortReassigned, PortRegistered, PortRemoved},
    ics24::{CONFIG_TTL_EXTEND_TO, CONFIG_TTL_THRESHOLD},
    identifiers::validate_port_id,
    types::DataKey,
    IbcRouter, IbcRouterArgs, IbcRouterClient,
};

pub trait Port {
    fn register_port(env: &Env, port_id: String, app_address: Address);

    fn reassign_port(env: &Env, port_id: String, app_address: Address);

    fn remove_port(env: &Env, port_id: String);

    fn port_app(env: &Env, port_id: String) -> Option<Address>;

    fn port_packets_in_flight(env: &Env, port_id: String) -> u64;
}

#[contractimpl]
impl Port for IbcRouter {
    #[only_owner]
    fn register_port(env: &Env, port_id: String, app_address: Address) {
        validate_port_id(env, &port_id);

        let key = DataKey::Port(port_id.clone());

        if env.storage().persistent().has(&key) {
            panic_with_error!(env, Error::PortAlreadyRegistered);
        }

        env.storage().persistent().set(&key, &app_address);

        PortRegistered {
            port_id,
            app: app_address,
        }
        .publish(env);
    }

    #[only_owner]
    fn reassign_port(env: &Env, port_id: String, app_address: Address) {
        validate_port_id(env, &port_id);

        let key = DataKey::Port(port_id.clone());
        let previous_app: Address = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, Error::PortNotRegistered));

        env.storage().persistent().set(&key, &app_address);

        PortReassigned {
            port_id,
            previous_app,
            app: app_address,
        }
        .publish(env);
    }

    #[only_owner]
    fn remove_port(env: &Env, port_id: String) {
        let key = DataKey::Port(port_id.clone());
        let previous_app: Address = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, Error::PortNotRegistered));

        if packets_in_flight(env, &port_id) > 0 {
            panic_with_error!(env, Error::PortHasPacketsInFlight);
        }

        env.storage().persistent().remove(&key);
        env.storage()
            .persistent()
            .remove(&DataKey::PortPacketsInFlight(port_id.clone()));

        PortRemoved {
            port_id,
            previous_app,
        }
        .publish(env);
    }

    fn port_app(env: &Env, port_id: String) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Port(port_id))
    }

    fn port_packets_in_flight(env: &Env, port_id: String) -> u64 {
        packets_in_flight(env, &port_id)
    }
}

fn packets_in_flight(env: &Env, port_id: &String) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::PortPacketsInFlight(port_id.clone()))
        .unwrap_or(0)
}

pub(crate) fn add_packet_in_flight(env: &Env, port_id: &String) {
    let key = DataKey::PortPacketsInFlight(port_id.clone());
    let count = packets_in_flight(env, port_id) + 1;
    let storage = env.storage().persistent();

    storage.set(&key, &count);
    storage.extend_ttl(&key, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
}

pub(crate) fn retire_packet_in_flight(env: &Env, port_id: &String) {
    let key = DataKey::PortPacketsInFlight(port_id.clone());
    let count = packets_in_flight(env, port_id).saturating_sub(1);
    let storage = env.storage().persistent();

    if count == 0 {
        storage.remove(&key);

        return;
    }

    storage.set(&key, &count);
    storage.extend_ttl(&key, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
}
