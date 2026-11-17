//! One-shot commands never start networking, so they must not ask an external
//! service for the node's address. The binary is run for real: the placement
//! of the lookup inside `run` is what is under test.

use std::io::Read;
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustock_core::config::ChainConfig;
use rustock_storage::epoch_store::{EpochConfig, EpochTrieStore};
use rustock_storage::BlockStore;

/// A database holding only the genesis block, as a node's first start leaves it.
fn seed_genesis(dir: &std::path::Path) {
    let config = ChainConfig::mainnet();
    let genesis = config.genesis_header();
    let hash = config
        .known_genesis_hash()
        .unwrap_or_else(|| genesis.hash());
    let store = BlockStore::open(dir).unwrap();
    store.put_header_with_hash(hash, &genesis).unwrap();
    store
        .put_total_difficulty(hash, genesis.difficulty)
        .unwrap();
    store.put_canonical_hash(0, hash).unwrap();
    store.set_head(hash).unwrap();
    drop(store);
    EpochTrieStore::open(dir.join("trie-epochs"), EpochConfig::default(), None).unwrap();
}

struct Outcome {
    success: bool,
    output: String,
    lookups: usize,
}

fn run_one_shot(data_dir: &std::path::Path, extra: &[&str]) -> Outcome {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicUsize::new(0));
    let (h, s) = (hits.clone(), stop.clone());
    let counter = std::thread::spawn(move || {
        while s.load(Ordering::SeqCst) == 0 {
            match listener.accept() {
                Ok(_) => {
                    h.fetch_add(1, Ordering::SeqCst);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    });

    let mut child = Command::new(env!("CARGO_BIN_EXE_rustock-cli"))
        .arg("--data-dir")
        .arg(data_dir)
        .args(["--trie-backend", "epoch", "--log-to-stdout"])
        .args(extra)
        .arg("--ip-check-url")
        .arg(format!("https://localhost:{port}/"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "one-shot command did not exit");
        std::thread::sleep(Duration::from_millis(50));
    };

    let mut output = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    stop.store(1, Ordering::SeqCst);
    counter.join().unwrap();
    Outcome {
        success: status.success(),
        output,
        lookups: hits.load(Ordering::SeqCst),
    }
}

#[test]
fn verify_state_reaches_its_own_error_without_a_lookup() {
    let dir = tempfile::tempdir().unwrap();
    seed_genesis(dir.path());
    let out = run_one_shot(dir.path(), &["--verify-state", "head"]);
    assert!(!out.success, "{}", out.output);
    assert!(
        out.output.contains("no executed head to verify"),
        "the command must get past node setup and fail on its own check:\n{}",
        out.output
    );
    assert_eq!(out.lookups, 0, "{}", out.output);
}

#[test]
fn measure_block_reads_makes_no_lookup() {
    let dir = tempfile::tempdir().unwrap();
    seed_genesis(dir.path());
    let out = run_one_shot(dir.path(), &["--measure-block-reads", "1"]);
    assert!(
        out.output.contains("Measuring trie reads"),
        "the command must get past node setup and start measuring:\n{}",
        out.output
    );
    assert_eq!(out.lookups, 0, "{}", out.output);
}
