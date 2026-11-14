//! Forks met the way rskj meets them, at the level of the sync service.
//!
//! Every test here fails on the code before `beside` existed: there a
//! competing block was not executed until the chain switched to it, a heavier
//! branch was adopted on its headers alone, and execution was pulled back to
//! the fork point to run the new branch again on a later tick.

use super::BesideFailure;
use crate::manager::SyncManager;
use crate::service::SyncService;
use crate::state::SyncState;
use crate::tests::dummy_header;
use alloy_primitives::{B256, B512, U256};
use rustock_core::events::ChainEvent;
use rustock_core::types::header::Header;
use rustock_core::validation::HeaderVerifier;
use rustock_core::Block;
use rustock_networking::protocol::{P2pMessage, RskMessage, RskSubMessage};
use rustock_storage::BlockStore;
use rustock_trie::{MemoryTrieStore, TrieNode, TrieStore};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const PEER: B512 = B512::repeat_byte(0x01);

/// A node that executes blocks, with one peer, and a stand-in for the block
/// processor that counts what it runs.
struct Node {
    service: SyncService,
    store: Arc<BlockStore>,
    /// How many times each block was executed beside the chain.
    runs: Arc<Mutex<HashMap<B256, usize>>>,
    /// Blocks the stand-in refuses as breaking consensus.
    invalid: Arc<Mutex<HashSet<B256>>>,
    /// Blocks the stand-in fails to run as a local fault would, and how many
    /// more times.
    local_faults: Arc<Mutex<HashMap<B256, usize>>>,
    /// What the node sent its peer.
    sent: mpsc::Receiver<P2pMessage>,
    /// What it told subscribers.
    events: tokio::sync::broadcast::Receiver<ChainEvent>,
    /// Every best block the miner was told of, in order.
    best: Arc<Mutex<Vec<B256>>>,
    /// The state root every header here names: the empty trie's, held.
    root: B256,
    _dir: tempfile::TempDir,
}

async fn node() -> Node {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(BlockStore::open(dir.path()).unwrap());
    let trie = Arc::new(MemoryTrieStore::new()) as Arc<dyn TrieStore>;
    let empty = TrieNode::empty();
    let root = empty.compute_hash(trie.as_ref());
    trie.put(root.as_slice(), &empty.to_message(trie.as_ref()));

    let peer_store = Arc::new(rustock_networking::peers::PeerStore::new());
    let (to_peer, sent) = mpsc::channel(rustock_networking::peers::PEER_CHANNEL_CAPACITY);
    peer_store.add_peer(PEER, to_peer).await;
    let manager = Arc::new(SyncManager::new(
        store.clone(),
        Arc::new(HeaderVerifier::new()),
        peer_store.clone(),
    ));
    let (_event_tx, event_rx) = mpsc::unbounded_channel();
    let processor = rustock_execution::BlockProcessor::new(
        rustock_execution::RskHardforkConfig::mainnet(),
        store.clone(),
    );
    let events = rustock_core::events::channel();
    let listeners = crate::BestBlockListeners::new();
    let best = Arc::new(Mutex::new(Vec::new()));
    let told = best.clone();
    listeners.add(Arc::new(move |header: &Header| {
        told.lock().unwrap().push(header.hash())
    }));
    let mut service = SyncService::new(manager, peer_store, event_rx)
        .with_block_processor(processor, trie, empty)
        .with_events(Some(events.clone()))
        .with_best_block_listeners(listeners);

    let runs = Arc::new(Mutex::new(HashMap::new()));
    let invalid = Arc::new(Mutex::new(HashSet::new()));
    let local_faults = Arc::new(Mutex::new(HashMap::new()));
    let (counted, refused, faulty) = (runs.clone(), invalid.clone(), local_faults.clone());
    service.set_test_beside(move |block: &Block| {
        let hash = block.hash();
        *counted.lock().unwrap().entry(hash).or_insert(0) += 1;
        if refused.lock().unwrap().contains(&hash) {
            return Err(BesideFailure::Invalid("refused by the test".into()));
        }
        if let Some(left) = faulty
            .lock()
            .unwrap()
            .get_mut(&hash)
            .filter(|left| **left > 0)
        {
            *left -= 1;
            return Err(BesideFailure::Local(
                "a database fault, made by the test".into(),
            ));
        }
        Ok(())
    });
    service.state = SyncState::Following;
    Node {
        service,
        store,
        runs,
        invalid,
        local_faults,
        sent,
        events: events.subscribe(),
        best,
        root,
        _dir: dir,
    }
}

impl Node {
    /// A header naming the one state root the trie holds; `mark` tells
    /// siblings apart.
    fn header(&self, number: u64, parent: B256, difficulty: u64, mark: u8) -> Header {
        let mut header = dummy_header(number, parent, U256::from(difficulty));
        header.state_root = self.root;
        header.extra_data = vec![mark].into();
        header
    }

    /// Genesis and the blocks above it, of the difficulties given, canonical
    /// with their totals and executed to the top. Genesis first.
    fn chain(&self, difficulties: &[u64]) -> Vec<Header> {
        let genesis = self.header(0, B256::ZERO, 1, 0);
        let mut total = 1u64;
        self.store.update_head(&genesis, U256::from(total)).unwrap();
        let mut chain = vec![genesis];
        for (i, difficulty) in difficulties.iter().enumerate() {
            let parent = chain.last().unwrap().hash();
            let header = self.header(i as u64 + 1, parent, *difficulty, 0xA0);
            total += difficulty;
            self.store.update_head(&header, U256::from(total)).unwrap();
            chain.push(header);
        }
        let top = chain.last().unwrap().hash();
        self.store.set_exec_head(top, self.root).unwrap();
        chain
    }

    /// A peer sends `header` as a whole block, with an empty body.
    async fn new_block(&mut self, header: &Header) {
        let block = Block {
            header: header.clone(),
            transactions: Vec::new(),
            ommers: Vec::new(),
        };
        self.service.on_new_block(PEER, block).await;
    }

    fn executed_head(&self) -> Option<B256> {
        self.store.exec_head().unwrap().map(|(hash, _)| hash)
    }

    fn head(&self) -> Option<B256> {
        self.store.head().unwrap()
    }

    fn runs(&self, hash: B256) -> usize {
        self.runs.lock().unwrap().get(&hash).copied().unwrap_or(0)
    }

    /// The hashes of the bodies and the headers the node has asked its peer
    /// for since last asked.
    fn asked(&mut self) -> (Vec<B256>, Vec<(B256, u32)>) {
        let (mut bodies, mut headers) = (Vec::new(), Vec::new());
        while let Ok(message) = self.sent.try_recv() {
            match message {
                P2pMessage::RskMessage(RskMessage {
                    sub_message: RskSubMessage::BodyRequest(request),
                    ..
                }) => bodies.push(request.hash),
                P2pMessage::RskMessage(RskMessage {
                    sub_message: RskSubMessage::BlockHeadersRequest(request),
                    ..
                }) => headers.push((request.query.hash, request.query.count)),
                _ => {}
            }
        }
        (bodies, headers)
    }

    /// The blocks announced to subscribers since last asked: each hash, and
    /// whether it was retracted.
    fn announced(&mut self) -> Vec<(B256, bool)> {
        let mut announced = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            if let ChainEvent::Block(block) = event {
                announced.push((block.hash, block.removed));
            }
        }
        announced
    }
}

/// rskj connects a competing block as it arrives, against its own parent's
/// state (`BlockChainImpl.internalTryToConnect`), and keeps its best block
/// while the competitor is lighter. Before, the block was stored for a miner
/// to name as an uncle and nothing more.
#[tokio::test]
async fn a_competing_block_is_executed_on_arrival_and_not_adopted_while_lighter() {
    let mut node = node().await;
    let chain = node.chain(&[1, 2]); // totals 2 and 4
    let (a1, a2) = (chain[1].hash(), chain[2].hash());

    let b2 = node.header(2, a1, 1, 0xB0); // total 3
    node.new_block(&b2).await;

    assert_eq!(
        node.runs(b2.hash()),
        1,
        "the competing block is executed as it arrives"
    );
    assert_eq!(
        node.head(),
        Some(a2),
        "and, lighter, it does not take the head"
    );
    assert_eq!(node.executed_head(), Some(a2), "nor execution");
    assert_eq!(node.store.canonical_hash(2).unwrap(), Some(a2));
    assert!(
        node.best.lock().unwrap().is_empty(),
        "the miner's best block did not change"
    );
}

/// A heavier branch whose blocks were executed as they came is switched to the
/// moment it outweighs the chain: in the call that delivers the block that
/// tips it, with nothing run again and no tick, grace period or download in
/// between. The miner and subscribers hear of it then too: the old block
/// retracted first, as rskj's logs emitter retracts it.
///
/// Before, the switch pulled execution back to the fork point, and the new
/// branch was run there once a later tick had fetched it.
#[tokio::test]
async fn a_heavier_branch_executed_as_it_came_is_switched_to_at_once() {
    let mut node = node().await;
    let chain = node.chain(&[1, 2]); // totals 2 and 4
    let (a1, a2) = (chain[1].hash(), chain[2].hash());
    let b2 = node.header(2, a1, 1, 0xB0); // total 3
    let b3 = node.header(3, b2.hash(), 2, 0xB0); // total 5

    node.new_block(&b2).await;
    node.new_block(&b3).await;

    assert_eq!(node.head(), Some(b3.hash()));
    assert_eq!(
        node.store.exec_head().unwrap(),
        Some((b3.hash(), node.root)),
        "execution is at the new tip, not the fork point"
    );
    assert_eq!(node.store.canonical_hash(2).unwrap(), Some(b2.hash()));
    assert_eq!(node.store.canonical_hash(3).unwrap(), Some(b3.hash()));
    assert_eq!(
        (node.runs(b2.hash()), node.runs(b3.hash())),
        (1, 1),
        "each block of the new branch ran once, as it arrived, and not again on the switch"
    );
    assert_eq!(node.store.deferred_head(), None);
    assert_eq!(
        *node.best.lock().unwrap(),
        vec![b3.hash()],
        "the miner builds on the new tip"
    );
    assert_eq!(
        node.announced(),
        vec![(a2, true), (b2.hash(), false), (b3.hash(), false)],
        "the block left is retracted, then the branch taken up is announced"
    );
}

/// A chain heavier by its headers whose bodies never come leaves the head,
/// the canonical chain and execution where they are, as rskj's best block
/// stays on what it has connected, and the node goes on following its own
/// chain. The bodies are asked for. Before, the headers alone took the head
/// and pulled execution back to the fork point, where withheld bodies held
/// the node for good.
#[tokio::test]
async fn heavier_headers_without_bodies_move_neither_the_head_nor_execution() {
    let mut node = node().await;
    let chain = node.chain(&[1, 2]); // totals 2 and 4
    let (a1, a2) = (chain[1].hash(), chain[2].hash());
    let x2 = node.header(2, a1, 1, 0xC0); // total 3
    let x3 = node.header(3, x2.hash(), 9, 0xC0); // total 12

    node.service
        .on_headers_response(PEER, vec![x3.clone(), x2.clone()])
        .await;

    assert_eq!(node.head(), Some(a2), "headers alone do not take the head");
    assert_eq!(node.executed_head(), Some(a2), "nor pull execution back");
    assert_eq!(node.store.canonical_hash(2).unwrap(), Some(a2));
    assert_eq!(
        node.store.deferred_head(),
        Some(x3.hash()),
        "the branch is held back"
    );
    let (bodies, _) = node.asked();
    for wanted in [x2.hash(), x3.hash()] {
        assert!(
            bodies.contains(&wanted),
            "the bodies of the branch are asked for"
        );
    }

    // Its own chain goes on.
    let a3 = node.header(3, a2, 1, 0xA0); // total 5
    node.new_block(&a3).await;
    assert_eq!(node.head(), Some(a3.hash()));
    assert_eq!(node.store.canonical_hash(2).unwrap(), Some(a2));
}

/// A branch held back whose bodies never come is given up once it has made no
/// progress for a minute, so that withholding them cannot keep the node from
/// syncing.
#[tokio::test]
async fn a_held_back_branch_whose_bodies_never_come_is_given_up() {
    let mut node = node().await;
    let chain = node.chain(&[1, 2]);
    let (a1, a2) = (chain[1].hash(), chain[2].hash());
    let x2 = node.header(2, a1, 1, 0xC0);
    let x3 = node.header(3, x2.hash(), 9, 0xC0);
    node.service
        .on_headers_response(PEER, vec![x3.clone(), x2])
        .await;
    assert_eq!(node.store.deferred_head(), Some(x3.hash()));

    node.service.advance_deferred_branch().await;
    assert_eq!(
        node.store.deferred_head(),
        Some(x3.hash()),
        "not given up at once"
    );

    let a_minute_ago = Instant::now()
        .checked_sub(Duration::from_secs(61))
        .expect("the clock has run a minute");
    node.service.deferred_watch = Some((x3.hash(), a_minute_ago));
    node.service.advance_deferred_branch().await;
    assert_eq!(node.store.deferred_head(), None, "given up");
    assert_eq!(node.head(), Some(a2));
    assert_eq!(node.executed_head(), Some(a2));
}

/// A block that breaks consensus is never adopted, however heavy the branch
/// built on it, and is not executed twice. rskj connects no block that
/// fails. Before, the branch was adopted on its headers.
#[tokio::test]
async fn an_invalid_block_beside_the_chain_is_never_adopted_however_heavy_its_branch() {
    let mut node = node().await;
    let chain = node.chain(&[1, 2]);
    let (a1, a2) = (chain[1].hash(), chain[2].hash());
    let y2 = node.header(2, a1, 1, 0xD0);
    let y3 = node.header(3, y2.hash(), 9, 0xD0); // total 12
    node.invalid.lock().unwrap().insert(y2.hash());

    node.new_block(&y2).await;
    node.new_block(&y3).await;
    node.service.advance_deferred_branch().await;
    node.service.take_up_stored_successors().await;

    assert_eq!(node.head(), Some(a2));
    assert_eq!(node.executed_head(), Some(a2));
    assert_eq!(
        node.store.deferred_head(),
        None,
        "a branch through it is given up"
    );
    assert_eq!(node.runs(y2.hash()), 1, "the invalid block ran once");
    assert_eq!(node.runs(y3.hash()), 0, "its child never ran");
}

/// A block this node failed to run for a fault of its own -- its database,
/// not the block -- is tried again, and its branch adopted once it runs. It
/// is not taken for invalid.
#[tokio::test]
async fn a_block_that_failed_for_a_local_fault_is_tried_again() {
    let mut node = node().await;
    let chain = node.chain(&[1, 2]);
    let a1 = chain[1].hash();
    let y2 = node.header(2, a1, 1, 0xD0);
    let y3 = node.header(3, y2.hash(), 9, 0xD0);
    node.local_faults.lock().unwrap().insert(y2.hash(), 1);

    node.new_block(&y2).await;
    assert_eq!(node.runs(y2.hash()), 1);
    node.new_block(&y3).await;

    assert_eq!(node.runs(y2.hash()), 2, "tried again");
    assert_eq!(
        node.executed_head(),
        Some(y3.hash()),
        "and adopted once it ran"
    );
}

/// The mainnet wedge of 2026-09-18, which held a node for three days: it had
/// executed a2 on a branch the network left, and held the winning branch's
/// headers -- b2, b3 -- without having adopted them.
///
/// The reconcile used to adopt the stored branch for being there and pull
/// execution back to the fork point, b1. Now the branch is weighed as rskj
/// weighs one: its bodies are fetched, it is executed beside the chain, and
/// the store adopts it with execution at its tip.
#[tokio::test]
async fn a_branch_stored_by_headers_alone_is_executed_beside_the_chain_and_adopted_at_its_tip() {
    let mut node = node().await;
    let chain = node.chain(&[1, 1]); // b1 total 2, a2 total 3
    let (b1, a2) = (chain[1].hash(), chain[2].hash());
    let b2 = node.header(2, b1, 5, 0xBB);
    let b3 = node.header(3, b2.hash(), 5, 0xBB);
    // As the store held them: headers, by height, with no pointer and no
    // total.
    node.store.put_header(&b2).unwrap();
    node.store.put_header(&b3).unwrap();

    node.service.reconcile_exec_head_with_canonical().await;
    assert_eq!(
        node.store.canonical_hash(3).unwrap(),
        None,
        "not adopted on its headers"
    );
    assert_eq!(
        node.executed_head(),
        Some(a2),
        "and execution not pulled back to b1"
    );

    node.service.take_up_stored_successors().await;
    let (bodies, _) = node.asked();
    assert!(
        bodies.contains(&b2.hash()) && bodies.contains(&b3.hash()),
        "its bodies are asked for"
    );

    // The bodies come, the child's first.
    let mut requests: Vec<(u64, B256)> = node
        .service
        .pending_branch_bodies
        .iter()
        .map(|(id, (hash, ..))| (*id, *hash))
        .collect();
    requests.sort_by_key(|(_, hash)| *hash != b3.hash());
    for (id, _) in requests {
        node.service
            .on_body_response(PEER, id, Vec::new(), Vec::new(), None)
            .await;
    }

    assert_eq!(node.head(), Some(b3.hash()));
    assert_eq!(
        node.store.exec_head().unwrap(),
        Some((b3.hash(), node.root))
    );
    assert_eq!(node.store.canonical_hash(2).unwrap(), Some(b2.hash()));
    assert_eq!((node.runs(b2.hash()), node.runs(b3.hash())), (1, 1));
}

/// The mainnet stalls of 2026-09-21 and 2026-09-24: the node held a block of
/// the network's branch but not its parent -- one block missing, then three.
/// The reconcile used to roll execution and the head back below the fork so
/// that the range would be downloaded again. Now nothing is rolled back: the
/// headers the branch lacks are asked for, ending at the first one missing,
/// once, and when they come the branch's bodies are fetched to execute it
/// beside the chain.
#[tokio::test]
async fn a_block_whose_ancestry_was_never_downloaded_has_it_asked_for_and_nothing_rolled_back() {
    for depth in [1u64, 3] {
        let mut node = node().await;
        // A common chain to the fork point, then ours, `depth` blocks above it.
        let fork_number = 30;
        let mut difficulties = vec![1; fork_number as usize];
        difficulties.extend(std::iter::repeat_n(1, depth as usize));
        let chain = node.chain(&difficulties);
        let fork = chain[fork_number as usize].hash();
        let ours = chain.last().unwrap().hash();
        let top = fork_number + depth;

        // The network's branch, never downloaded, and a child of it that was.
        let mut theirs = Vec::new();
        let mut parent = fork;
        for number in fork_number + 1..=top {
            let header = node.header(number, parent, 9, 0xCC);
            parent = header.hash();
            theirs.push(header);
        }
        let child = node.header(top + 1, parent, 9, 0xCC);
        node.store.put_header(&child).unwrap();

        node.service.reconcile_exec_head_with_canonical().await;
        node.service.take_up_stored_successors().await;
        node.service.take_up_stored_successors().await;

        assert_eq!(
            node.executed_head(),
            Some(ours),
            "depth {depth}: execution stays"
        );
        assert_eq!(node.head(), Some(ours), "depth {depth}: the head stays");
        let (_, headers) = node.asked();
        assert_eq!(
            headers,
            vec![(parent, top as u32)],
            "depth {depth}: the headers ending at the missing parent are asked for, once"
        );

        // They come, newest first.
        let mut answer = theirs.clone();
        answer.reverse();
        node.service.on_headers_response(PEER, answer).await;
        assert_eq!(
            node.executed_head(),
            Some(ours),
            "depth {depth}: still not on headers"
        );
        let (bodies, _) = node.asked();
        for header in &theirs {
            assert!(
                bodies.contains(&header.hash()),
                "depth {depth}: the bodies of #{} are asked for",
                header.number
            );
        }
    }
}
