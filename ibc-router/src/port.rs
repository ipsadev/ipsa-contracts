use soroban_sdk::{contractimpl, panic_with_error, Address, Env, String};
use stellar_macros::only_owner;

use crate::{
    errors::Error, identifiers::validate_port_id, types::DataKey, IbcRouter, IbcRouterArgs,
    IbcRouterClient,
};

pub trait Port {
    fn register_port(env: &Env, port_id: String, app_address: Address);

    fn reassign_port(env: &Env, port_id: String, app_address: Address);

    fn port_app(env: &Env, port_id: String) -> Option<Address>;
}

#[contractimpl]
impl Port for IbcRouter {
    #[only_owner]
    fn register_port(env: &Env, port_id: String, app_address: Address) {
        validate_port_id(env, &port_id);
        let key = DataKey::Port(port_id);
        if env.storage().persistent().has(&key) {
            panic_with_error!(env, Error::PortAlreadyRegistered);
        }
        env.storage().persistent().set(&key, &app_address);
    }

    #[only_owner]
    fn reassign_port(env: &Env, port_id: String, app_address: Address) {
        let key = DataKey::Port(port_id);
        if !env.storage().persistent().has(&key) {
            panic_with_error!(env, Error::PortNotRegistered);
        }
        env.storage().persistent().set(&key, &app_address);
    }

    fn port_app(env: &Env, port_id: String) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Port(port_id))
    }
}
