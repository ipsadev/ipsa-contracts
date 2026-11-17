use soroban_sdk::{testutils::Address as _, xdr::ToXdr, Address, Bytes, BytesN, Env, String};
use stellar_ibc_router::{IbcRouter, IbcRouterClient};
use stellar_tendermint_light_client::{
    Height, TendermintClientState, TendermintConsensusState, TendermintLightClient,
    TendermintLightClientClient, TrustThreshold,
};

const CLIENT_TYPE: &str = "07-tendermint";

struct Net {
    env: Env,
    router: IbcRouterClient<'static>,
    router_address: Address,
}

fn setup() -> Net {
    let env = Env::default();
    let owner = Address::generate(&env);
    let router_address = env.register(IbcRouter, (owner,));
    let router = IbcRouterClient::new(&env, &router_address);

    env.mock_all_auths();

    let light_client = deploy_light_client(&env, &router_address);

    router.register_client_type(&String::from_str(&env, CLIENT_TYPE), &light_client);

    env.set_auths(&[]);

    Net {
        env,
        router,
        router_address,
    }
}

fn deploy_light_client(env: &Env, router: &Address) -> Address {
    env.register(TendermintLightClient, (router.clone(),))
}

fn client_state(env: &Env, height: u64) -> Bytes {
    TendermintClientState {
        chain_id: String::from_str(env, "simd-1"),
        trust_level: TrustThreshold {
            numerator: 1,
            denominator: 3,
        },
        trusting_period_secs: 1_000_000,
        unbonding_period_secs: 2_000_000,
        max_clock_drift_secs: 10,
        latest_height: Height {
            revision_number: 0,
            revision_height: height,
        },
        is_frozen: false,
        frozen_height: Height {
            revision_number: 0,
            revision_height: 0,
        },
        proof_specs: Bytes::new(env),
    }
    .to_xdr(env)
}

fn consensus_state(env: &Env, root: u8) -> Bytes {
    TendermintConsensusState {
        timestamp_secs: 1,
        next_validators_hash: BytesN::from_array(env, &[0; 32]),
        root: BytesN::from_array(env, &[root; 32]),
    }
    .to_xdr(env)
}

fn create(net: &Net) -> (String, TendermintLightClientClient<'static>) {
    let client_id = net.router.create_client(
        &String::from_str(&net.env, CLIENT_TYPE),
        &client_state(&net.env, 10),
        &consensus_state(&net.env, 0xaa),
        &10,
    );

    let address = net
        .router
        .light_client_address(&client_id)
        .expect("created");

    (
        client_id,
        TendermintLightClientClient::new(&net.env, &address),
    )
}

#[test]
fn the_router_initialises_a_client_without_anyone_else_signing() {
    let net = setup();

    let (client_id, light_client) = create(&net);

    assert_eq!(light_client.latest_height(&client_id), 10);
}

#[test]
fn a_stranger_cannot_overwrite_a_live_client() {
    let net = setup();
    let (client_id, light_client) = create(&net);

    let result = light_client.try_initialise(
        &client_id,
        &client_state(&net.env, 99),
        &consensus_state(&net.env, 0xbb),
        &99,
    );

    assert!(result.is_err());
    assert_eq!(light_client.latest_height(&client_id), 10);
    assert_eq!(
        light_client.consensus_state(&client_id, &10),
        consensus_state(&net.env, 0xaa)
    );
}

#[test]
fn a_stranger_cannot_claim_the_next_client_id() {
    let net = setup();
    let light_client_address = net
        .router
        .client_type_address(&String::from_str(&net.env, CLIENT_TYPE))
        .expect("registered");
    let light_client = TendermintLightClientClient::new(&net.env, &light_client_address);
    let next_id = String::from_str(&net.env, "07-tendermint-0");

    let result = light_client.try_initialise(
        &next_id,
        &client_state(&net.env, 99),
        &consensus_state(&net.env, 0xbb),
        &99,
    );

    assert!(result.is_err());

    let (client_id, light_client) = create(&net);

    assert_eq!(client_id, next_id);
    assert_eq!(light_client.latest_height(&client_id), 10);
}

#[test]
fn a_client_id_cannot_be_initialised_twice_even_by_the_router() {
    let net = setup();
    let (client_id, light_client) = create(&net);

    net.env.mock_all_auths();

    let result = light_client.try_initialise(
        &client_id,
        &client_state(&net.env, 99),
        &consensus_state(&net.env, 0xbb),
        &99,
    );

    assert!(result.is_err());
    assert_eq!(light_client.latest_height(&client_id), 10);
}

#[test]
fn a_stranger_cannot_freeze_a_client() {
    let net = setup();
    let (client_id, light_client) = create(&net);

    let result = light_client.try_update_state_on_misbehaviour(&client_id, &Bytes::new(&net.env));

    assert!(result.is_err());
    assert!(!light_client.frozen(&client_id));
}

#[test]
fn migration_initialises_the_replacement_through_the_router() {
    let net = setup();
    let (client_id, original) = create(&net);
    let replacement_address = deploy_light_client(&net.env, &net.router_address);
    let replacement = TendermintLightClientClient::new(&net.env, &replacement_address);

    net.env.mock_all_auths();

    net.router.migrate_client(
        &client_id,
        &replacement_address,
        &client_state(&net.env, 20),
        &consensus_state(&net.env, 0xcc),
        &20,
    );

    assert_eq!(
        net.router.light_client_address(&client_id),
        Some(replacement_address)
    );
    assert_eq!(replacement.latest_height(&client_id), 20);
    assert_eq!(original.latest_height(&client_id), 10);
}

#[test]
fn a_replacement_cannot_be_seeded_by_a_stranger_before_migration() {
    let net = setup();
    let (client_id, _original) = create(&net);
    let replacement_address = deploy_light_client(&net.env, &net.router_address);
    let replacement = TendermintLightClientClient::new(&net.env, &replacement_address);

    let result = replacement.try_initialise(
        &client_id,
        &client_state(&net.env, 99),
        &consensus_state(&net.env, 0xbb),
        &99,
    );

    assert!(result.is_err());
}
