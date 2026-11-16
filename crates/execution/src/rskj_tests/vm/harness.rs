//! What the ported tests of the VM share: programs written as rskj's tests
//! write them, and a chain of blocks for them to run in.
//!
//! rskj's VM tests write a program as opcode names, compiled by their
//! `BytecodeCompiler`, and step it on a mocked program or run it in blocks
//! of their DSL. Here a program is the code of a contract a transaction
//! calls, or the init code a transaction deploys, in a block rustock's
//! executor runs: the path every block takes.

use std::sync::Arc;

use alloy_primitives::{Address, Bloom, Bytes, B256, U256};
use rustock_core::{Header, Transaction};
use rustock_storage::BlockStore;
use rustock_trie::{AccountState, MemoryTrieStore, TrieKeySlice, TrieNode};

use crate::executor::{BlockExecutionResult, ExecutionError, RskExecutor, TxExecutionResult};
use crate::hardfork::RskHardforkConfig;

/// The account that sends the transactions of a test.
pub(super) const SENDER: Address = Address::repeat_byte(0xa1);
/// The contract a test's program runs as.
pub(super) const CONTRACT: Address = Address::repeat_byte(0xc0);
/// One RBTC, in wei.
pub(super) const ONE_RBTC: U256 = U256::from_limbs([1_000_000_000_000_000_000, 0, 0, 0]);
/// The gas a test's transaction is given, unless it says otherwise.
pub(super) const GAS: u64 = 1_000_000;
/// What [`return_top`] of one word costs: two PUSH2s, an MSTORE and the
/// word of memory it takes, and a PUSH1. RETURN costs nothing.
pub(super) const RETURN_ONE_WORD_GAS: u64 = 15;

/// Compiles `code` as rskj's `BytecodeCompiler` does: each word is an
/// opcode's name, in any case, or `0x` and bytes in hex, which go in as
/// they are. rskj's names are read as rskj reads them: DUPN, SWAPN and
/// TXINDEX are its own opcodes (0xa8 to 0xaa), not EIP-8024's.
pub(super) fn compile(code: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for token in code.split_whitespace() {
        let token = token.to_ascii_uppercase();
        match token.strip_prefix("0X") {
            Some(hex) => bytes.extend(alloy_primitives::hex::decode(hex).expect("hex bytes")),
            None => bytes.push(opcode(&token).unwrap_or_else(|| panic!("no opcode {token}"))),
        }
    }
    bytes
}

fn opcode(name: &str) -> Option<u8> {
    match name {
        "DUPN" => Some(0xa8),
        "SWAPN" => Some(0xa9),
        "TXINDEX" => Some(0xaa),
        "SHA3" => Some(0x20),
        "SUICIDE" => Some(0xff),
        _ => (0..=255u8)
            .find(|byte| revm::bytecode::OpCode::new(*byte).is_some_and(|op| op.as_str() == name)),
    }
}

/// Code that ends a program by returning the top `words` words of its
/// stack, the top one first.
pub(super) fn return_top(words: usize) -> Vec<u8> {
    let mut code = Vec::new();
    for word in 0..words {
        code.push(0x61); // PUSH2
        code.extend(((word * 32) as u16).to_be_bytes());
        code.push(0x52); // MSTORE
    }
    code.push(0x61); // PUSH2
    code.extend(((words * 32) as u16).to_be_bytes());
    code.extend([0x60, 0x00, 0xf3]); // PUSH1 0, RETURN
    code
}

/// The words of a program's output.
pub(super) fn words(output: &[u8]) -> Vec<U256> {
    output.chunks(32).map(U256::from_be_slice).collect()
}

/// A transaction calling `to` with `data`, at a gas price of 0.
pub(super) fn call(nonce: u64, to: Address, value: U256, gas: u64, data: &[u8]) -> Transaction {
    Transaction {
        nonce,
        gas_price: U256::ZERO,
        gas_limit: U256::from(gas),
        to: Bytes::copy_from_slice(to.as_slice()),
        value,
        input: Bytes::copy_from_slice(data),
        v: 0,
        r: U256::ZERO,
        s: U256::ZERO,
        cached_rlp: None,
    }
}

/// A chain on one network's rules: its state, and the number of the next
/// block, which each block run moves on by one.
pub(super) struct Chain {
    executor: RskExecutor,
    store: Arc<MemoryTrieStore>,
    root: TrieNode,
    /// The next block's number.
    pub(super) number: u64,
    _blocks: tempfile::TempDir,
}

impl Chain {
    pub(super) fn new(config: RskHardforkConfig, number: u64) -> Self {
        let blocks = tempfile::tempdir().expect("a folder for the block store");
        let block_store = Arc::new(BlockStore::open(blocks.path()).expect("a block store"));
        Self {
            executor: RskExecutor::new(config, block_store),
            store: Arc::new(MemoryTrieStore::new()),
            root: TrieNode::empty(),
            number,
            _blocks: blocks,
        }
    }

    /// A chain on mainnet's rules whose next block is `number`.
    pub(super) fn mainnet(number: u64) -> Self {
        Self::new(RskHardforkConfig::mainnet(), number)
    }

    /// Writes an account with `nonce` and `balance`, and `code` where it
    /// has any.
    pub(super) fn put(&mut self, address: Address, nonce: u64, balance: U256, code: &[u8]) {
        let store = self.store.as_ref();
        let account = AccountState::new(U256::from(nonce), balance).encode();
        let key = TrieKeySlice::from_key(&rustock_trie::account_key(&address));
        self.root = self.root.put(&key, &account, store);
        if !code.is_empty() {
            let key = TrieKeySlice::from_key(&rustock_trie::code_key(&address));
            self.root = self.root.put(&key, code, store);
        }
    }

    /// The next block's header.
    pub(super) fn header(&self) -> Header {
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
            number: self.number,
            gas_limit: U256::from(6_800_000),
            gas_used: 0,
            timestamp: 1_700_000_000,
            extra_data: Bytes::new(),
            paid_fees: U256::ZERO,
            minimum_gas_price: U256::ZERO,
            uncle_count: 0,
            umm_root: rustock_core::config::ActivationHeights::current()
                .umm_active(self.number)
                .then(Bytes::new),
            version_fields: None,
            bitcoin_merged_mining_header: None,
            bitcoin_merged_mining_merkle_proof: None,
            bitcoin_merged_mining_coinbase_transaction: None,
            cached_hash: None,
            cached_hash_for_merged_mining: None,
            kept: None,
        }
    }

    /// Runs `transactions` in the next block, or says what refuses the block.
    /// A block that runs leaves its state for the next.
    pub(super) fn try_block(
        &mut self,
        transactions: &[(Transaction, Address)],
    ) -> Result<BlockExecutionResult, ExecutionError> {
        let result = self.executor.execute_block(
            &self.header(),
            transactions,
            &self.root,
            self.store.clone(),
        )?;
        self.root = match &result.applied_root {
            Some(root) => root.clone(),
            None => crate::state::apply_state_changes(
                &self.root,
                self.store.as_ref(),
                &result.state_changes,
                &result.markers,
            ),
        };
        self.number += 1;
        Ok(result)
    }

    /// Runs `transactions` in the next block.
    pub(super) fn block(
        &mut self,
        transactions: &[(Transaction, Address)],
    ) -> BlockExecutionResult {
        self.try_block(transactions).expect("the block runs")
    }

    /// Runs `transaction` from `sender`, alone in the next block.
    pub(super) fn run(&mut self, sender: Address, transaction: Transaction) -> TxExecutionResult {
        self.block(&[(transaction, sender)]).tx_results.remove(0)
    }

    /// The account at `address`, where there is one.
    pub(super) fn account(&self, address: Address) -> Option<AccountState> {
        let key = TrieKeySlice::from_key(&rustock_trie::account_key(&address));
        let encoded = self.root.get(&key, self.store.as_ref())?;
        Some(AccountState::decode(&encoded).expect("an account"))
    }

    /// The balance at `address`: zero where there is no account.
    pub(super) fn balance(&self, address: Address) -> U256 {
        self.account(address)
            .map_or(U256::ZERO, |account| account.balance)
    }
}

/// What a transaction of [`GAS`] gets from calling a contract whose code is
/// `code`, alone in block `number` of a chain on `config`'s rules.
pub(super) fn run_code_on(
    config: RskHardforkConfig,
    number: u64,
    code: &[u8],
) -> TxExecutionResult {
    let mut chain = Chain::new(config, number);
    chain.put(SENDER, 0, ONE_RBTC, &[]);
    chain.put(CONTRACT, 1, U256::ZERO, code);
    chain.run(SENDER, call(0, CONTRACT, U256::ZERO, GAS, &[]))
}

/// [`run_code_on`] mainnet's rules.
pub(super) fn run_code(number: u64, code: &[u8]) -> TxExecutionResult {
    run_code_on(RskHardforkConfig::mainnet(), number, code)
}
