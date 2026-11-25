#![no_std]

pub mod hash;
pub mod tree;

pub use hash::{empty, inner_hash, is_empty, key_index, leaf_hash, TREE_DEPTH};
pub use tree::{compute_root, Branch, Child, Tree, TreeStorage};
