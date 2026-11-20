use ipsa_smt::{empty, Tree};
use soroban_sdk::{BytesN, Env};

use crate::{
    events::IbcRoot,
    smt::storage::{extend, RouterStorage, SmtKey},
};

pub(crate) fn commit(env: &Env) -> BytesN<32> {
    let tree_storage = RouterStorage::new(env);
    let root = Tree::new(env, &tree_storage).root();
    let storage = env.storage().persistent();

    storage.set(&SmtKey::Root, &root);

    extend(env, &SmtKey::Root);

    if storage.has(&SmtKey::Top) {
        extend(env, &SmtKey::Top);
    }

    IbcRoot { root: root.clone() }.publish(env);

    root
}

pub(crate) fn stored_root(env: &Env) -> BytesN<32> {
    env.storage()
        .persistent()
        .get(&SmtKey::Root)
        .unwrap_or_else(|| empty(env))
}
