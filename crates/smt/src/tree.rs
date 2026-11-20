use soroban_sdk::{contracttype, BytesN, Env, Map, Vec};

use crate::hash::{empty, inner_hash, is_empty, TREE_DEPTH};

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Child {
    Leaf(u64),
    Node(u32, u64),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub hash: BytesN<32>,
    pub left: Child,
    pub right: Child,
}

pub trait TreeStorage {
    fn top(&self) -> Option<Child>;
    fn set_top(&self, top: Option<Child>);
    fn branch(&self, level: u32, prefix: u64) -> Option<Branch>;
    fn set_branch(&self, level: u32, prefix: u64, branch: &Branch);
    fn remove_branch(&self, level: u32, prefix: u64);
    fn leaf(&self, index: u64) -> Option<BytesN<32>>;
    fn has_leaf(&self, index: u64) -> bool;
    fn set_leaf(&self, index: u64, hash: &BytesN<32>);
    fn remove_leaf(&self, index: u64);
    fn corrupt(&self) -> !;
}

fn position(child: &Child) -> (u32, u64) {
    match child {
        Child::Leaf(index) => (0, *index),
        Child::Node(level, prefix) => (*level, *prefix),
    }
}

fn contains(child: &Child, index: u64) -> bool {
    let (level, prefix) = position(child);

    if level >= TREE_DEPTH {
        return true;
    }

    index >> level == prefix
}

fn split_level(index: u64, existing: &Child) -> u32 {
    let (level, prefix) = position(existing);
    let base = prefix.checked_shl(level).unwrap_or(0);
    let differing = base ^ index;

    TREE_DEPTH - differing.leading_zeros()
}

fn lift(env: &Env, hash: BytesN<32>, from: u32, prefix: u64, to: u32) -> BytesN<32> {
    let empty = empty(env);
    let mut hash = hash;

    for level in from..to {
        let at_level = prefix >> (level - from);
        hash = if at_level & 1 == 0 {
            inner_hash(env, &hash, &empty)
        } else {
            inner_hash(env, &empty, &hash)
        };
    }

    hash
}

pub struct Tree<'a, S: TreeStorage> {
    env: &'a Env,
    storage: &'a S,
}

impl<'a, S: TreeStorage> Tree<'a, S> {
    pub fn new(env: &'a Env, storage: &'a S) -> Self {
        Self { env, storage }
    }

    pub fn set(&self, index: u64, hash: &BytesN<32>) {
        let existed = self.storage.has_leaf(index);

        self.storage.set_leaf(index, hash);

        let (path, reached) = self.descend(index);

        if existed {
            self.rehash(&path);

            return;
        }

        let leaf = Child::Leaf(index);

        let Some(existing) = reached else {
            self.storage.set_top(Some(leaf));

            return;
        };

        let level = split_level(index, &existing);
        let prefix = index.checked_shr(level).unwrap_or(0);
        let (left, right) = if (index >> (level - 1)) & 1 == 0 {
            (leaf, existing.clone())
        } else {
            (existing.clone(), leaf)
        };
        let branch = Branch {
            hash: self.branch_hash(level, &left, &right),
            left,
            right,
        };

        self.storage.set_branch(level, prefix, &branch);
        self.replace_child(&path, &existing, Some(Child::Node(level, prefix)));
        self.rehash(&path);
    }

    pub fn remove(&self, index: u64) {
        if !self.storage.has_leaf(index) {
            return;
        }

        self.storage.remove_leaf(index);

        let (mut path, reached) = self.descend(index);

        if reached != Some(Child::Leaf(index)) {
            self.storage.corrupt();
        }

        let Some((level, prefix)) = path.pop_back() else {
            self.storage.set_top(None);

            return;
        };

        let parent = self.load_branch(level, prefix);
        let sibling = if parent.left == Child::Leaf(index) {
            parent.right
        } else {
            parent.left
        };

        self.storage.remove_branch(level, prefix);
        self.replace_child(&path, &Child::Node(level, prefix), Some(sibling));
        self.rehash(&path);
    }

    pub fn root(&self) -> BytesN<32> {
        match self.storage.top() {
            None => empty(self.env),
            Some(child) => self.lifted_child(&child, TREE_DEPTH),
        }
    }

    fn load_branch(&self, level: u32, prefix: u64) -> Branch {
        match self.storage.branch(level, prefix) {
            Some(branch) => branch,
            None => self.storage.corrupt(),
        }
    }

    fn child_hash(&self, child: &Child) -> BytesN<32> {
        match child {
            Child::Leaf(index) => match self.storage.leaf(*index) {
                Some(hash) => hash,
                None => self.storage.corrupt(),
            },
            Child::Node(level, prefix) => self.load_branch(*level, *prefix).hash,
        }
    }

    fn lifted_child(&self, child: &Child, to: u32) -> BytesN<32> {
        let (level, prefix) = position(child);

        lift(self.env, self.child_hash(child), level, prefix, to)
    }

    fn branch_hash(&self, level: u32, left: &Child, right: &Child) -> BytesN<32> {
        inner_hash(
            self.env,
            &self.lifted_child(left, level - 1),
            &self.lifted_child(right, level - 1),
        )
    }

    fn descend(&self, index: u64) -> (Vec<(u32, u64)>, Option<Child>) {
        let mut path = Vec::new(self.env);
        let mut current = self.storage.top();

        while let Some(child) = current.clone() {
            let Child::Node(level, prefix) = child else {
                break;
            };

            if !contains(&child, index) {
                break;
            }

            let branch = self.load_branch(level, prefix);
            path.push_back((level, prefix));
            current = Some(if (index >> (level - 1)) & 1 == 0 {
                branch.left
            } else {
                branch.right
            });
        }

        (path, current)
    }

    fn replace_child(&self, path: &Vec<(u32, u64)>, old: &Child, new: Option<Child>) {
        let Some((level, prefix)) = path.last() else {
            self.storage.set_top(new);

            return;
        };

        let mut branch = self.load_branch(level, prefix);
        let replacement = match new {
            Some(child) => child,
            None => self.storage.corrupt(),
        };

        if branch.left == *old {
            branch.left = replacement;
        } else if branch.right == *old {
            branch.right = replacement;
        } else {
            self.storage.corrupt();
        }

        self.storage.set_branch(level, prefix, &branch);
    }

    fn rehash(&self, path: &Vec<(u32, u64)>) {
        for (level, prefix) in path.iter().rev() {
            let mut branch = self.load_branch(level, prefix);
            branch.hash = self.branch_hash(level, &branch.left, &branch.right);
            self.storage.set_branch(level, prefix, &branch);
        }
    }
}

pub fn compute_root(env: &Env, leaves: &Map<u64, BytesN<32>>) -> BytesN<32> {
    if leaves.is_empty() {
        return empty(env);
    }

    let mut level = leaves.clone();

    for _ in 0..TREE_DEPTH {
        let mut next: Map<u64, BytesN<32>> = Map::new(env);

        for (index, _) in level.iter() {
            let parent = index >> 1;

            if next.contains_key(parent) {
                continue;
            }

            let left = level.get(parent << 1).unwrap_or_else(|| empty(env));
            let right = level.get((parent << 1) | 1).unwrap_or_else(|| empty(env));
            let folded = inner_hash(env, &left, &right);

            if !is_empty(&folded) {
                next.set(parent, folded);
            }
        }

        level = next;
    }

    level.get(0).unwrap_or_else(|| empty(env))
}
