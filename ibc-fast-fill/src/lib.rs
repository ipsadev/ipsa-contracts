#![no_std]

use ipsa::ics20::TransferReceived;
use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, vec, Address, Bytes, BytesN, Env, IntoVal,
    String, Symbol,
};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;

mod errors;
mod events;
mod order;
mod storage;
mod types;

pub use crate::{
    errors::Error,
    events::{OrderFilled, OrderSettled, RefundNotified},
    types::{Config, Fill, Order},
};

#[contract]
pub struct FastFill;

#[contractimpl(contracttrait)]
impl Ownable for FastFill {}

#[contractimpl]
impl FastFill {
    pub fn __constructor(env: Env, owner: Address, config: Config) {
        set_owner(&env, &owner);
        storage::set_config(&env, &config);
    }

    #[only_owner]
    pub fn configure(env: Env, config: Config) {
        storage::set_config(&env, &config);
    }

    #[only_owner]
    pub fn set_route(env: Env, dest_client: String, denom: String, token: Address) {
        storage::set_route(&env, &dest_client, &denom, &token);
    }

    #[only_owner]
    pub fn remove_route(env: Env, dest_client: String, denom: String) {
        storage::remove_route(&env, &dest_client, &denom);
    }

    #[only_owner]
    pub fn upgrade(env: Env, wasm_hash: BytesN<32>) {
        env.deployer().update_current_contract_wasm(wasm_hash);
    }

    pub fn config(env: Env) -> Config {
        storage::config(&env)
    }

    pub fn route(env: Env, dest_client: String, denom: String) -> Option<Address> {
        storage::route(&env, &dest_client, &denom)
    }

    pub fn order_key(env: Env, order: Order) -> BytesN<32> {
        order::key(&env, &order)
    }

    pub fn fill_of(env: Env, key: BytesN<32>) -> Option<Fill> {
        storage::fill(&env, &key)
    }

    pub fn is_settled(env: Env, key: BytesN<32>) -> bool {
        storage::settled(&env, &key)
    }

    pub fn fill(env: Env, provider: Address, order: Order, provider_evm: String) -> BytesN<32> {
        provider.require_auth();

        storage::extend_instance(&env);

        if order.amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }

        let terms = order::terms(&env, &order.memo);

        if terms.fee < 0 || terms.fee >= order.amount {
            panic_with_error!(&env, Error::InvalidFee);
        }

        if env.ledger().timestamp() > terms.deadline {
            panic_with_error!(&env, Error::FillDeadlinePassed);
        }

        if !order::addressed_to(&order.memo, &env.current_contract_address()) {
            panic_with_error!(&env, Error::NotAddressedHere);
        }

        order::evm_address(&env, &provider_evm);

        let token = storage::route(&env, &order.dest_client, &order.denom)
            .unwrap_or_else(|| panic_with_error!(&env, Error::UnknownRoute));

        let key = order::key(&env, &order);

        if storage::settled(&env, &key) {
            panic_with_error!(&env, Error::AlreadySettled);
        }

        if storage::fill(&env, &key).is_some() {
            panic_with_error!(&env, Error::AlreadyFilled);
        }

        let paid = order.amount - terms.fee;

        token::Client::new(&env, &token).transfer(&provider, &terms.recipient, &paid);

        storage::set_fill(
            &env,
            &key,
            &Fill {
                provider: provider.clone(),
                provider_evm,
                recipient: terms.recipient.clone(),
                token: token.clone(),
                dest_client: order.dest_client.clone(),
                sender: order.sender.clone(),
                filled_at: env.ledger().sequence(),
            },
        );

        OrderFilled {
            key: key.clone(),
            source_client: order.source_client,
            sequence: order.sequence,
            provider,
            recipient: terms.recipient,
            token,
            paid,
        }
        .publish(&env);

        key
    }

    pub fn on_ibc_transfer_received(env: Env, transfer_app: Address, received: TransferReceived) {
        transfer_app.require_auth();

        storage::extend_instance(&env);

        if transfer_app != storage::config(&env).transfer_app {
            panic_with_error!(&env, Error::Unauthorized);
        }

        let filled = storage::denom(&env, &received.dest_client, &received.token).map(|denom| {
            let key = order::key(
                &env,
                &Order {
                    source_client: received.source_client.clone(),
                    dest_client: received.dest_client.clone(),
                    sequence: received.sequence,
                    sender: received.sender.clone(),
                    denom,
                    amount: received.amount,
                    memo: received.memo.clone(),
                },
            );

            let matching = storage::fill(&env, &key).filter(|fill| {
                fill.token == received.token
                    && fill.dest_client == received.dest_client
                    && fill.sender == received.sender
            });

            (key, matching)
        });

        let (key, paid_to, was_filled) = match filled {
            Some((key, Some(fill))) if !storage::settled(&env, &key) => {
                (Some(key), fill.provider, true)
            }
            Some((key, _)) => (
                Some(key),
                order::terms(&env, &received.memo).recipient,
                false,
            ),
            None => (None, order::terms(&env, &received.memo).recipient, false),
        };

        token::Client::new(&env, &received.token).transfer(
            &env.current_contract_address(),
            &paid_to,
            &received.amount,
        );

        if let Some(key) = key {
            storage::set_settled(&env, &key);

            OrderSettled {
                key,
                paid_to,
                token: received.token,
                amount: received.amount,
                filled: was_filled,
            }
            .publish(&env);
        }
    }

    pub fn notify_refund(env: Env, key: BytesN<32>) -> u64 {
        storage::extend_instance(&env);

        let fill =
            storage::fill(&env, &key).unwrap_or_else(|| panic_with_error!(&env, Error::NotFilled));

        if storage::settled(&env, &key) {
            panic_with_error!(&env, Error::AlreadySettled);
        }

        if storage::notified(&env, &key) {
            panic_with_error!(&env, Error::AlreadyNotified);
        }

        storage::set_notified(&env, &key);

        let config = storage::config(&env);

        let provider_evm = order::evm_address(&env, &fill.provider_evm);

        let payload = order::assign_refund_call(&env, &key, &provider_evm);

        let timeout = env.ledger().timestamp() + config.notice_timeout_seconds;

        let gmp_sequence = env.invoke_contract::<u64>(
            &config.gmp_app,
            &Symbol::new(&env, "send_call"),
            vec![
                &env,
                env.current_contract_address().into_val(&env),
                fill.dest_client.into_val(&env),
                fill.sender.into_val(&env),
                Bytes::new(&env).into_val(&env),
                payload.into_val(&env),
                timeout.into_val(&env),
                String::from_str(&env, "").into_val(&env),
                config.notice_encoding.into_val(&env),
            ],
        );

        RefundNotified {
            key,
            provider_evm: fill.provider_evm,
            gmp_sequence,
        }
        .publish(&env);

        gmp_sequence
    }
}
