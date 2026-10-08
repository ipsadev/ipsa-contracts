extern crate alloc;

use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction},
    Address, Bytes, BytesN, Env, String,
};
use stellar_sp1_ethereum_light_client::{Sp1EthereumLightClient, Sp1EthereumLightClientClient};

const STELLAR: &str = include_str!("./fixtures/proof-fixture.json");
const HEADER: &str = include_str!("./fixtures/ethereum-proof.json");
const MEMBERSHIP: &str = include_str!("./fixtures/ethereum-membership-proof.json");

const OTHER_PROGRAM: [u8; 32] = [0x99; 32];

fn from(source: &str, name: &str) -> alloc::vec::Vec<u8> {
    let key = alloc::format!("\"{name}\": \"0x");
    let start = source.find(&key).expect("field missing") + key.len();
    let end = source[start..].find('"').expect("unterminated") + start;

    hex(&source[start..end])
}

fn hex(hexed: &str) -> alloc::vec::Vec<u8> {
    (0..hexed.len() / 2)
        .map(|i| u8::from_str_radix(&hexed[i * 2..i * 2 + 2], 16).unwrap())
        .collect()
}

fn vkey_of(source: &str) -> [u8; 32] {
    from(source, "vkey").try_into().expect("not 32 bytes")
}

fn header_hash(offset: usize) -> [u8; 32] {
    from(HEADER, "publicValues")[offset..offset + 32]
        .try_into()
        .unwrap()
}

fn header_u64(offset: usize) -> u64 {
    let mut raw = [0_u8; 8];

    raw.copy_from_slice(&from(HEADER, "publicValues")[offset..offset + 8]);

    u64::from_be_bytes(raw)
}

fn client_state(env: &Env, vkey: &[u8], membership: &[u8], commitment: &[u8]) -> Bytes {
    let mut out = Bytes::new(env);

    out.extend_from_slice(vkey);
    out.extend_from_slice(membership);
    out.extend_from_slice(commitment);

    out
}

fn consensus_state(env: &Env, hash: &[u8], timestamp: u64) -> Bytes {
    let mut out = Bytes::new(env);

    out.extend_from_slice(hash);
    out.extend_from_array(&timestamp.to_be_bytes());

    out
}

fn message(env: &Env, public_values: &[u8], proof: &[u8]) -> Bytes {
    let mut out = Bytes::new(env);

    out.extend_from_array(&(public_values.len() as u32).to_be_bytes());
    out.extend_from_slice(public_values);
    out.extend_from_slice(proof);

    out
}

struct Fixed {
    env: Env,
    client: Sp1EthereumLightClientClient<'static>,
    router: Address,
    id: String,
}

fn deploy_without_auths() -> Fixed {
    let env = Env::default();
    let router = Address::generate(&env);
    let address = env.register(Sp1EthereumLightClient, (router.clone(),));

    Fixed {
        client: Sp1EthereumLightClientClient::new(&env, &address),
        router,
        id: String::from_str(&env, "08-wasm-0"),
        env,
    }
}

fn deploy() -> Fixed {
    let f = deploy_without_auths();

    f.env.mock_all_auths();

    f
}

fn deploy_for_header() -> Fixed {
    let f = deploy();

    f.client.initialise(
        &f.id,
        &client_state(
            &f.env,
            &vkey_of(HEADER),
            &vkey_of(MEMBERSHIP),
            &header_hash(0),
        ),
        &consensus_state(&f.env, &header_hash(32), 1),
        &header_u64(96),
    );

    f
}

fn header_message(f: &Fixed) -> Bytes {
    message(
        &f.env,
        &from(HEADER, "publicValues"),
        &from(HEADER, "proof"),
    )
}

#[test]
fn only_the_router_can_initialise() {
    let f = deploy_without_auths();

    let result = f.client.try_initialise(
        &f.id,
        &client_state(
            &f.env,
            &vkey_of(HEADER),
            &vkey_of(MEMBERSHIP),
            &header_hash(0),
        ),
        &consensus_state(&f.env, &header_hash(32), 1),
        &header_u64(96),
    );

    assert!(result.is_err());
    assert_eq!(f.client.latest_height(&f.id), 0);
}

#[test]
fn initialise_is_authorised_by_the_router() {
    let f = deploy_for_header();
    let auths = f.env.auths();

    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, f.router);
    assert!(matches!(
        &auths[0].1.function,
        AuthorizedFunction::Contract((_, name, _)) if name == &soroban_sdk::Symbol::new(&f.env, "initialise")
    ));
}

#[test]
#[should_panic(expected = "already initialised")]
fn a_client_id_cannot_be_initialised_twice() {
    let f = deploy_for_header();

    f.client.initialise(
        &f.id,
        &client_state(&f.env, &vkey_of(HEADER), &OTHER_PROGRAM, &header_hash(0)),
        &consensus_state(&f.env, &header_hash(32), 1),
        &header_u64(96),
    );
}

#[test]
fn initialise_stores_what_it_was_given() {
    let f = deploy_for_header();

    assert_eq!(f.client.latest_height(&f.id), header_u64(96));
    assert_eq!(
        f.client.consensus_hash(&f.id, &header_u64(96)),
        BytesN::from_array(&f.env, &header_hash(32))
    );
    assert_eq!(f.client.get_timestamp_at_height(&f.id, &header_u64(96)), 1);
    assert!(!f.client.frozen(&f.id));
}

#[test]
#[should_panic(expected = "must be 96 bytes")]
fn a_client_state_of_the_wrong_length_is_refused() {
    let f = deploy();

    f.client.initialise(
        &f.id,
        &Bytes::from_array(&f.env, &[0_u8; 64]),
        &consensus_state(&f.env, &header_hash(32), 1),
        &header_u64(96),
    );
}

#[test]
#[should_panic(expected = "must be 40 bytes")]
fn a_consensus_state_of_the_wrong_length_is_refused() {
    let f = deploy();

    f.client.initialise(
        &f.id,
        &client_state(&f.env, &vkey_of(HEADER), &OTHER_PROGRAM, &header_hash(0)),
        &Bytes::from_array(&f.env, &[0_u8; 32]),
        &header_u64(96),
    );
}

#[test]
#[should_panic(expected = "does not verify")]
fn a_proof_of_another_program_is_refused() {
    let f = deploy();

    f.client.initialise(
        &f.id,
        &client_state(&f.env, &OTHER_PROGRAM, &OTHER_PROGRAM, &header_hash(0)),
        &consensus_state(&f.env, &header_hash(32), 1),
        &header_u64(96),
    );

    f.client.update_state(&f.id, &header_message(&f));
}

#[test]
#[should_panic(expected = "not the expected length")]
fn a_real_proof_of_the_wrong_shape_fails_on_the_layout() {
    let f = deploy();

    f.client.initialise(
        &f.id,
        &client_state(&f.env, &vkey_of(STELLAR), &OTHER_PROGRAM, &header_hash(0)),
        &consensus_state(&f.env, &header_hash(32), 1),
        &header_u64(96),
    );

    f.client.update_state(
        &f.id,
        &message(
            &f.env,
            &from(STELLAR, "publicValues"),
            &from(STELLAR, "proof"),
        ),
    );
}

#[test]
#[should_panic(expected = "no length prefix")]
fn a_message_with_no_length_prefix_is_refused() {
    let f = deploy_for_header();

    f.client
        .update_state(&f.id, &Bytes::from_array(&f.env, &[0x00, 0x01]));
}

#[test]
#[should_panic(expected = "shorter than its length prefix")]
fn a_message_that_lies_about_its_length_is_refused() {
    let f = deploy_for_header();

    f.client.update_state(
        &f.id,
        &Bytes::from_array(&f.env, &[0x00, 0x00, 0xff, 0xff, 0x01]),
    );
}

#[test]
fn a_real_ethereum_header_advances_the_client() {
    let f = deploy_for_header();

    let slot = f.client.update_state(&f.id, &header_message(&f));

    assert_eq!(slot, header_u64(104), "advanced to the proven slot");
    assert_eq!(
        f.client.consensus_hash(&f.id, &slot),
        BytesN::from_array(&f.env, &header_hash(64)),
        "the stored consensus state is the one the proof established"
    );
    assert_eq!(f.client.latest_height(&f.id), slot);
    assert_eq!(
        f.client.get_timestamp_at_height(&f.id, &slot),
        header_u64(112)
    );
}

#[test]
#[should_panic(expected = "different client parameters")]
fn a_proof_against_other_client_parameters_is_refused() {
    let f = deploy();

    let mut commitment = header_hash(0);

    commitment[0] ^= 0x01;

    f.client.initialise(
        &f.id,
        &client_state(&f.env, &vkey_of(HEADER), &OTHER_PROGRAM, &commitment),
        &consensus_state(&f.env, &header_hash(32), 1),
        &header_u64(96),
    );

    f.client.update_state(&f.id, &header_message(&f));
}

#[test]
#[should_panic(expected = "does not hold")]
fn a_proof_from_an_unknown_trusted_state_is_refused() {
    let f = deploy();

    let mut trusted = header_hash(32);

    trusted[0] ^= 0x01;

    f.client.initialise(
        &f.id,
        &client_state(&f.env, &vkey_of(HEADER), &OTHER_PROGRAM, &header_hash(0)),
        &consensus_state(&f.env, &trusted, 1),
        &header_u64(96),
    );

    f.client.update_state(&f.id, &header_message(&f));
}

#[test]
fn a_real_packet_is_proven_against_a_header_this_client_verified() {
    let f = deploy_for_header();

    let slot = f.client.update_state(&f.id, &header_message(&f));

    let path = Bytes::from_slice(&f.env, &hex("636c69656e742d30010000000000000001"));
    let value = Bytes::from_slice(
        &f.env,
        &hex("220361e106d47194caa25cfbb015ff5da1db5912c521c219e71a4e838e7a9a86"),
    );

    let proven = f.client.verify_membership(
        &f.id,
        &slot,
        &message(
            &f.env,
            &from(MEMBERSHIP, "publicValues"),
            &from(MEMBERSHIP, "proof"),
        ),
        &path,
        &value,
    );

    assert!(proven, "the packet is in the state the header established");
}

#[test]
#[should_panic(expected = "different path or value")]
fn a_membership_proof_of_another_packet_is_refused() {
    let f = deploy_for_header();

    let slot = f.client.update_state(&f.id, &header_message(&f));

    let path = Bytes::from_slice(&f.env, &hex("636c69656e742d30010000000000000002"));
    let value = Bytes::from_slice(
        &f.env,
        &hex("220361e106d47194caa25cfbb015ff5da1db5912c521c219e71a4e838e7a9a86"),
    );

    f.client.verify_membership(
        &f.id,
        &slot,
        &message(
            &f.env,
            &from(MEMBERSHIP, "publicValues"),
            &from(MEMBERSHIP, "proof"),
        ),
        &path,
        &value,
    );
}
