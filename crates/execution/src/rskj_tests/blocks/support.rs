//! What the ported tests of a block's run share.

use crate::executor::{ExecutionError, RskExecutor, TxExecutionResult};
use crate::hardfork::RskHardforkConfig;
use alloy_primitives::{Address, Bloom, Bytes, B256, U256};
use rustock_core::{Header, Transaction};
use rustock_storage::BlockStore;
use rustock_trie::{account_key, AccountState, MemoryTrieStore, TrieKeySlice, TrieNode};
use std::sync::Arc;

/// Mainnet's lovell700, where RSKIP438 activates.
pub(super) const LOVELL700: u64 = 7_338_024;

/// The account that sends the transactions run here.
pub(super) const SENDER: Address = Address::repeat_byte(0x5e);

/// A header at `number`, with nothing a lone transaction's run reads but
/// its height, its gas limit and its coinbase.
pub(super) fn header_at(number: u64) -> Header {
    Header {
        parent_hash: B256::ZERO,
        ommers_hash: B256::ZERO,
        beneficiary: Address::repeat_byte(0x01),
        state_root: B256::ZERO,
        transactions_root: B256::ZERO,
        receipts_root: B256::ZERO,
        logs_bloom: Bloom::ZERO,
        extension_data: None,
        difficulty: U256::from(1_000_000),
        number,
        gas_limit: U256::from(6_800_000),
        gas_used: 0,
        timestamp: 1_700_000_000,
        extra_data: Bytes::new(),
        paid_fees: U256::ZERO,
        minimum_gas_price: U256::ZERO,
        uncle_count: 0,
        umm_root: None,
        version_fields: None,
        bitcoin_merged_mining_header: None,
        bitcoin_merged_mining_merkle_proof: None,
        bitcoin_merged_mining_coinbase_transaction: None,
        cached_hash: None,
        cached_hash_for_merged_mining: None,
        kept: None,
    }
}

/// A creation carrying `init_code`, priced at nothing, with a gas limit
/// of 1,000,000. Unsigned: who sends it is given where it runs.
pub(super) fn creation(init_code: Vec<u8>) -> Transaction {
    Transaction {
        nonce: 0,
        gas_price: U256::ZERO,
        gas_limit: U256::from(1_000_000),
        to: Bytes::new(),
        value: U256::ZERO,
        input: Bytes::from(init_code),
        v: 0,
        r: U256::ZERO,
        s: U256::ZERO,
        cached_rlp: None,
    }
}

/// `transaction`, from [`SENDER`] holding one RBTC, run alone on mainnet at
/// block `number`.
pub(super) fn run_on_mainnet(
    number: u64,
    transaction: &Transaction,
) -> Result<TxExecutionResult, ExecutionError> {
    let store = Arc::new(MemoryTrieStore::new());
    let key = account_key(&SENDER);
    let funded = AccountState::new(U256::ZERO, U256::from(10u64).pow(U256::from(18)));
    let root = TrieNode::empty().put(
        &TrieKeySlice::from_key(&key),
        &funded.encode(),
        store.as_ref(),
    );
    let blocks_dir = tempfile::tempdir().unwrap();
    let blocks = Arc::new(BlockStore::open(blocks_dir.path()).unwrap());
    RskExecutor::new(RskHardforkConfig::mainnet(), blocks).execute_tx(
        &header_at(number),
        transaction,
        SENDER,
        &root,
        store,
    )
}
