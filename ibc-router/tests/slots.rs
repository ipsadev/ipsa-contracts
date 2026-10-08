use soroban_sdk::{BytesN, Env, Map};
use stellar_ibc_router::smt::{claim_slot, release_slot};

const SLOT: u64 = 42;

fn hash(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn occupied(env: &Env, owner: Option<u8>) -> (Map<u64, BytesN<32>>, Map<u64, BytesN<32>>) {
    let mut leaves = Map::new(env);
    let mut owners = Map::new(env);

    leaves.set(SLOT, hash(env, 0xee));

    if let Some(owner) = owner {
        owners.set(SLOT, hash(env, owner));
    }

    (leaves, owners)
}

#[test]
fn a_free_slot_is_claimed_by_the_first_key() {
    let env = Env::default();
    let mut owners = Map::new(&env);

    assert!(claim_slot(&mut owners, SLOT, &hash(&env, 0x01)));
    assert_eq!(owners.get(SLOT), Some(hash(&env, 0x01)));
}

#[test]
fn the_owner_can_overwrite_its_own_slot() {
    let env = Env::default();
    let (_leaves, mut owners) = occupied(&env, Some(0x01));

    assert!(claim_slot(&mut owners, SLOT, &hash(&env, 0x01)));
}

#[test]
fn a_colliding_key_cannot_take_an_occupied_slot() {
    let env = Env::default();
    let (_leaves, mut owners) = occupied(&env, Some(0x01));

    assert!(!claim_slot(&mut owners, SLOT, &hash(&env, 0x02)));
    assert_eq!(
        owners.get(SLOT),
        Some(hash(&env, 0x01)),
        "the owner is unchanged"
    );
}

#[test]
fn a_colliding_key_cannot_remove_another_keys_leaf() {
    let env = Env::default();
    let (mut leaves, mut owners) = occupied(&env, Some(0x01));

    assert!(!release_slot(
        &mut leaves,
        &mut owners,
        SLOT,
        &hash(&env, 0x02)
    ));
    assert!(
        leaves.contains_key(SLOT),
        "the receipt or commitment survives"
    );
    assert_eq!(owners.get(SLOT), Some(hash(&env, 0x01)));
}

#[test]
fn the_owner_removes_its_leaf_and_frees_the_slot() {
    let env = Env::default();
    let (mut leaves, mut owners) = occupied(&env, Some(0x01));

    assert!(release_slot(
        &mut leaves,
        &mut owners,
        SLOT,
        &hash(&env, 0x01)
    ));
    assert!(!leaves.contains_key(SLOT));
    assert!(owners.get(SLOT).is_none());
    assert!(claim_slot(&mut owners, SLOT, &hash(&env, 0x02)));
}

#[test]
fn removing_an_empty_slot_changes_nothing() {
    let env = Env::default();
    let mut leaves = Map::new(&env);
    let mut owners = Map::new(&env);

    assert!(!release_slot(
        &mut leaves,
        &mut owners,
        SLOT,
        &hash(&env, 0x01)
    ));
}

#[test]
fn a_leaf_written_before_owners_were_recorded_is_still_removable() {
    let env = Env::default();
    let (mut leaves, mut owners) = occupied(&env, None);

    assert!(release_slot(
        &mut leaves,
        &mut owners,
        SLOT,
        &hash(&env, 0x01)
    ));
    assert!(!leaves.contains_key(SLOT));
}
