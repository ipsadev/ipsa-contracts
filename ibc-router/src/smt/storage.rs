use ipsa_smt::{Branch, Child, TreeStorage};
use soroban_sdk::{contracttype, panic_with_error, BytesN, Env};

use crate::{
    errors::Error,
    ics24::{CONFIG_TTL_EXTEND_TO, CONFIG_TTL_THRESHOLD},
};

#[contracttype]
#[derive(Clone)]
pub enum SmtKey {
    Root,
    Top,
    Leaf(u64),
    Owner(u64),
    Node(u32, u64),
}

pub(crate) fn extend(env: &Env, key: &SmtKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, CONFIG_TTL_THRESHOLD, CONFIG_TTL_EXTEND_TO);
}

pub(crate) struct RouterStorage<'a> {
    env: &'a Env,
}

impl<'a> RouterStorage<'a> {
    pub(crate) fn new(env: &'a Env) -> Self {
        Self { env }
    }
}

impl TreeStorage for RouterStorage<'_> {
    fn top(&self) -> Option<Child> {
        self.env.storage().persistent().get(&SmtKey::Top)
    }

    fn set_top(&self, top: Option<Child>) {
        let storage = self.env.storage().persistent();

        match top {
            Some(child) => {
                storage.set(&SmtKey::Top, &child);
                extend(self.env, &SmtKey::Top);
            }
            None => storage.remove(&SmtKey::Top),
        }
    }

    fn branch(&self, level: u32, prefix: u64) -> Option<Branch> {
        let key = SmtKey::Node(level, prefix);
        let branch = self.env.storage().persistent().get(&key);

        if branch.is_some() {
            extend(self.env, &key);
        }

        branch
    }

    fn set_branch(&self, level: u32, prefix: u64, branch: &Branch) {
        let key = SmtKey::Node(level, prefix);

        self.env.storage().persistent().set(&key, branch);
        extend(self.env, &key);
    }

    fn remove_branch(&self, level: u32, prefix: u64) {
        self.env
            .storage()
            .persistent()
            .remove(&SmtKey::Node(level, prefix));
    }

    fn leaf(&self, index: u64) -> Option<BytesN<32>> {
        self.env.storage().persistent().get(&SmtKey::Leaf(index))
    }

    fn has_leaf(&self, index: u64) -> bool {
        self.env.storage().persistent().has(&SmtKey::Leaf(index))
    }

    fn set_leaf(&self, index: u64, hash: &BytesN<32>) {
        let key = SmtKey::Leaf(index);

        self.env.storage().persistent().set(&key, hash);
        extend(self.env, &key);
    }

    fn remove_leaf(&self, index: u64) {
        self.env.storage().persistent().remove(&SmtKey::Leaf(index));
    }

    fn corrupt(&self) -> ! {
        panic_with_error!(self.env, Error::ProvableStoreCorrupt)
    }
}
