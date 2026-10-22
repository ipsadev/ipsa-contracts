use soroban_sdk::{
    contractimpl, panic_with_error, Address, Bytes, Env, IntoVal, String, Symbol, Val, Vec,
};
use stellar_macros::only_owner;

use crate::{
    errors::Error,
    events::{
        ClientCreated, ClientFrozen, ClientMigrated, ClientTypeRegistered, ClientUpdated,
        CounterpartyRegistered,
    },
    identifiers::{validate_client_type, validate_counterparty_client_id},
    types::{Counterparty, DataKey},
    IbcRouter, IbcRouterArgs, IbcRouterClient,
};

pub trait Ics02 {
    fn register_client_type(env: &Env, client_type: String, light_client_address: Address);

    fn client_type_address(env: &Env, client_type: String) -> Option<Address>;

    fn create_client(
        env: &Env,
        client_type: String,
        client_state: Bytes,
        consensus_state: Bytes,
        height: u64,
    ) -> String;

    fn register_counterparty(
        env: &Env,
        client_id: String,
        counterparty_client_id: String,
        counterparty_commitment_prefix: Vec<Bytes>,
    );

    fn counterparty(env: &Env, client_id: String) -> Option<Counterparty>;

    fn update_client(env: &Env, client_id: String, client_message: Bytes) -> u64;

    fn light_client_address(env: &Env, client_id: String) -> Option<Address>;

    fn migrate_client(
        env: &Env,
        client_id: String,
        light_client_address: Address,
        client_state: Bytes,
        consensus_state: Bytes,
        height: u64,
    );

    fn frozen(env: &Env, client_id: String) -> bool;
}

#[contractimpl]
impl Ics02 for IbcRouter {
    #[only_owner]
    fn register_client_type(env: &Env, client_type: String, light_client_address: Address) {
        validate_client_type(env, &client_type);

        env.storage().persistent().set(
            &DataKey::ClientTypeAddr(client_type.clone()),
            &light_client_address,
        );

        ClientTypeRegistered {
            client_type,
            light_client: light_client_address,
        }
        .publish(env);
    }

    fn client_type_address(env: &Env, client_type: String) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::ClientTypeAddr(client_type))
    }

    fn create_client(
        env: &Env,
        client_type: String,
        client_state: Bytes,
        consensus_state: Bytes,
        height: u64,
    ) -> String {
        let light_client_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::ClientTypeAddr(client_type.clone()))
            .unwrap_or_else(|| panic_with_error!(env, Error::UnknownClientType));

        let client_id = create_client_id(env, &client_type);

        let args: Vec<Val> = soroban_sdk::vec![
            env,
            client_id.clone().into_val(env),
            client_state.into_val(env),
            consensus_state.into_val(env),
            height.into_val(env),
        ];
        let _: () =
            env.invoke_contract(&light_client_address, &Symbol::new(env, "initialise"), args);

        env.storage()
            .persistent()
            .set(&DataKey::ClientType(client_id.clone()), &client_type);
        env.storage().persistent().set(
            &DataKey::LightClient(client_id.clone()),
            &light_client_address,
        );

        ClientCreated {
            client_id: client_id.clone(),
            client_type,
            height,
        }
        .publish(env);

        client_id
    }

    #[only_owner]
    fn register_counterparty(
        env: &Env,
        client_id: String,
        counterparty_client_id: String,
        counterparty_commitment_prefix: Vec<Bytes>,
    ) {
        validate_counterparty_client_id(env, &counterparty_client_id);
        if !env
            .storage()
            .persistent()
            .has(&DataKey::ClientType(client_id.clone()))
        {
            panic_with_error!(env, Error::ClientIdNotFound);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Counterparty(client_id.clone()))
        {
            panic_with_error!(env, Error::CounterpartyAlreadyRegistered);
        }
        let cp = Counterparty {
            client_id: counterparty_client_id.clone(),
            commitment_prefix: counterparty_commitment_prefix,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Counterparty(client_id.clone()), &cp);

        CounterpartyRegistered {
            client_id,
            counterparty_client_id,
        }
        .publish(env);
    }

    fn counterparty(env: &Env, client_id: String) -> Option<Counterparty> {
        env.storage()
            .persistent()
            .get(&DataKey::Counterparty(client_id))
    }

    fn update_client(env: &Env, client_id: String, client_message: Bytes) -> u64 {
        if env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Frozen(client_id.clone()))
            .unwrap_or(false)
        {
            panic_with_error!(env, Error::ClientFrozen);
        }

        let light_client_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::LightClient(client_id.clone()))
            .unwrap_or_else(|| panic_with_error!(env, Error::ClientIdNotFound));

        let misbehaviour_args: Vec<Val> = soroban_sdk::vec![
            env,
            client_id.clone().into_val(env),
            client_message.clone().into_val(env),
        ];
        let misbehaviour: bool = env.invoke_contract(
            &light_client_address,
            &Symbol::new(env, "check_for_misbehaviour"),
            misbehaviour_args,
        );

        if misbehaviour {
            let args: Vec<Val> = soroban_sdk::vec![
                env,
                client_id.clone().into_val(env),
                client_message.into_val(env),
            ];
            let _: () = env.invoke_contract(
                &light_client_address,
                &Symbol::new(env, "update_state_on_misbehaviour"),
                args,
            );
            env.storage()
                .persistent()
                .set(&DataKey::Frozen(client_id.clone()), &true);

            ClientFrozen { client_id }.publish(env);

            return 0;
        }

        let args: Vec<Val> = soroban_sdk::vec![
            env,
            client_id.clone().into_val(env),
            client_message.into_val(env),
        ];
        let height: u64 = env.invoke_contract(
            &light_client_address,
            &Symbol::new(env, "update_state"),
            args,
        );

        ClientUpdated { client_id, height }.publish(env);

        height
    }

    fn light_client_address(env: &Env, client_id: String) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::LightClient(client_id))
    }

    #[only_owner]
    fn migrate_client(
        env: &Env,
        client_id: String,
        light_client_address: Address,
        client_state: Bytes,
        consensus_state: Bytes,
        height: u64,
    ) {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::ClientType(client_id.clone()))
        {
            panic_with_error!(env, Error::ClientIdNotFound);
        }

        let initialise_args: Vec<Val> = soroban_sdk::vec![
            env,
            client_id.clone().into_val(env),
            client_state.into_val(env),
            consensus_state.into_val(env),
            height.into_val(env),
        ];
        let _: () = env.invoke_contract(
            &light_client_address,
            &Symbol::new(env, "initialise"),
            initialise_args,
        );

        let latest_args: Vec<Val> = soroban_sdk::vec![env, client_id.clone().into_val(env)];
        let latest_height: u64 = env.invoke_contract(
            &light_client_address,
            &Symbol::new(env, "latest_height"),
            latest_args,
        );

        if latest_height == 0 {
            panic_with_error!(env, Error::LightClientNotInitialised);
        }

        env.storage().persistent().set(
            &DataKey::LightClient(client_id.clone()),
            &light_client_address,
        );
        env.storage()
            .persistent()
            .remove(&DataKey::Frozen(client_id.clone()));

        ClientMigrated {
            client_id,
            light_client: light_client_address,
        }
        .publish(env);
    }

    fn frozen(env: &Env, client_id: String) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Frozen(client_id))
            .unwrap_or(false)
    }
}

fn create_client_id(env: &Env, client_type: &String) -> String {
    let key = DataKey::NextClientId(client_type.clone());
    let n: u32 = env.storage().persistent().get(&key).unwrap_or(0);
    env.storage().persistent().set(&key, &(n + 1));

    let prefix_len = client_type.len() as usize;
    let mut buf = [0u8; 64];
    client_type.copy_into_slice(&mut buf[..prefix_len]);
    let mut len = prefix_len;
    buf[len] = b'-';
    len += 1;
    len += write_u32(&mut buf[len..], n);

    String::from_bytes(env, &buf[..len])
}

fn write_u32(buf: &mut [u8], mut n: u32) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    i
}
