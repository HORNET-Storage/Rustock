//! rskj `co.rsk.validators.BlockUnclesValidationRuleTest`: an uncle is a
//! block below the one that names it, never its sibling or descendant
//! (`BlockUnclesValidationRule.validateParentNumber`).

use super::support::header;
use crate::types::block::Block;
use crate::types::header::Header;
use crate::validation::uncles::{AncestorSource, UnclesValidationRule};
use crate::validation::{HeaderValidator, ParentHeaderValidator, ValidationError};
use alloy_primitives::{Address, B256};
use std::collections::HashMap;

/// The chain as a block store holds it: headers by hash, none with uncles.
#[derive(Default)]
struct Store(HashMap<B256, Header>);

impl Store {
    fn put(&mut self, header: &Header) {
        self.0.insert(header.hash(), header.clone());
    }
}

impl AncestorSource for Store {
    fn header(&self, hash: B256) -> Option<Header> {
        self.0.get(&hash).cloned()
    }

    fn uncles_of(&self, _hash: B256) -> Vec<Header> {
        Vec::new()
    }
}

/// A child of `parent`, told apart from its siblings by its `miner`.
fn child_of(parent: &Header, miner: u8) -> Header {
    let mut child = header(parent.number + 1, parent.timestamp + 10);
    child.parent_hash = parent.hash();
    child.beneficiary = Address::repeat_byte(miner);
    child
}

/// rskj's chain: genesis and block 1 stored, and block 2, on block 1,
/// naming `uncle`. Uncle and generation limits of ten and no rule on the
/// uncle's own header, as rskj's test builds its rule.
fn judge(store: &Store, block_1: &Header, uncle: Header) -> Result<(), ValidationError> {
    let block = Block {
        header: child_of(block_1, 0xb0),
        transactions: Vec::new(),
        ommers: vec![uncle],
    };
    let header_rules: [Box<dyn HeaderValidator>; 0] = [];
    let parent_rules: [Box<dyn ParentHeaderValidator>; 0] = [];
    UnclesValidationRule {
        store,
        uncle_list_limit: 10,
        uncle_generation_limit: 10,
        header_rules: &header_rules,
        parent_rules: &parent_rules,
        fork_detection: None,
    }
    .validate(&block)
}

fn genesis_and_block_1() -> (Store, Header, Header) {
    let genesis = header(0, 1_000);
    let block_1 = child_of(&genesis, 0xa0);
    let mut store = Store::default();
    store.put(&genesis);
    store.put(&block_1);
    (store, genesis, block_1)
}

/// rskj `rejectBlockWithSiblingUncle`: block 2 can't name another child of
/// block 1, its sibling. A child of genesis, block 1's sibling, it can.
#[test]
fn a_sibling_of_the_block_is_no_uncle() {
    let (store, genesis, block_1) = genesis_and_block_1();
    let sibling = child_of(&block_1, 0xc0);
    assert_eq!(
        judge(&store, &block_1, sibling.clone()),
        Err(ValidationError::UncleIsSiblingOrDescendant {
            hash: sibling.hash()
        })
    );

    let parents_sibling = child_of(&genesis, 0xc1);
    assert_eq!(judge(&store, &block_1, parents_sibling), Ok(()));
}

/// rskj `rejectBlockWithUncleHavingHigherNumber`: block 2 can't name block 3,
/// a grandchild of block 1.
#[test]
fn a_block_above_the_one_naming_it_is_no_uncle() {
    let (store, _genesis, block_1) = genesis_and_block_1();
    let sibling = child_of(&block_1, 0xc0);
    let higher = child_of(&sibling, 0xc2);
    assert_eq!(higher.number, 3);
    assert_eq!(
        judge(&store, &block_1, higher.clone()),
        Err(ValidationError::UncleIsSiblingOrDescendant {
            hash: higher.hash()
        })
    );
}
