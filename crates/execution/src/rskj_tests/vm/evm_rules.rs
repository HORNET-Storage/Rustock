//! The EVM's rules as rskj's `VM`, `Program` and `TransactionExecutor` hold
//! them, where no ported test holds a rule or one side of its activation:
//! each case runs as a contract's code in a block, on each side of the height
//! rskj's `reference.conf` gives the rule.

use alloy_primitives::{Address, B256, U256};

use super::harness::{
    call, compile, push_address, return_top, run_code, words, Chain, CONTRACT, GAS, ONE_RBTC,
    SENDER,
};

/// orchid on mainnet: RSKIP91.
const ORCHID: u64 = 729_000;
/// wasabi100 on mainnet: RSKIP125.
const WASABI: u64 = 1_591_000;
/// iris300 on mainnet.
const IRIS: u64 = 3_614_800;
/// The contract a case's contract calls.
const CALLEE: Address = Address::repeat_byte(0xce);

/// RSKIP91 brings STATICCALL. Before it rskj's `VM` throws for the opcode as
/// for one it doesn't have (`OP_STATICCALL`), and the frame fails with all
/// its gas; from it the call runs, and pushes 1 for a callee with no code.
/// Before RSKIP103 (orchid060) STATICCALL takes a value, so the program gives
/// it seven words on both sides.
#[test]
fn staticcall_is_an_invalid_opcode_before_rskip91() {
    let static_call =
        compile("PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x2b GAS STATICCALL");
    let code = [static_call, return_top(1)].concat();

    let before = run_code(ORCHID - 1, &code);
    assert!(!before.success, "#{}", ORCHID - 1);
    assert_eq!(before.gas_used, GAS, "#{}", ORCHID - 1);

    let from = run_code(ORCHID, &code);
    assert!(from.success, "#{ORCHID}");
    assert_eq!(words(&from.output), [U256::from(1)], "#{ORCHID}");
}

/// In a static call, rskj's `VM.getMessageCall` refuses a CALL that moves a
/// value, and `doCREATE2` refuses CREATE2: the callee fails, and the STATICCALL
/// pushes 0. A CALLCODE that moves a value is no CALL, and runs: rskj checks
/// the opcode, not the value. The same callee run by a CALL does all three.
/// The callee holds 1,000 wei, so that only the static call can refuse it.
#[test]
fn in_a_static_call_a_call_with_a_value_and_create2_fail_and_callcode_with_one_runs() {
    let to = push_address(Address::repeat_byte(0x2b));
    let callee_codes = [
        format!("PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x01 {to} PUSH2 0xffff CALL"),
        format!(
            "PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x01 {to} PUSH2 0xffff CALLCODE"
        ),
        "PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 CREATE2".to_string(),
    ];
    let callee = push_address(CALLEE);
    let zeros = "PUSH1 0x00 PUSH1 0x00 PUSH1 0x00 PUSH1 0x00";
    let statically = compile(&format!("{zeros} {callee} PUSH3 0x030d40 STATICCALL"));
    let plainly = compile(&format!("{zeros} PUSH1 0x00 {callee} PUSH3 0x030d40 CALL"));
    let run = |caller: &[u8], callee_code: &str| {
        let mut chain = Chain::mainnet(IRIS);
        chain.put(SENDER, 0, ONE_RBTC, &[]);
        chain.put(CONTRACT, 1, U256::ZERO, &[caller, &return_top(1)].concat());
        chain.put(CALLEE, 1, U256::from(1_000), &compile(callee_code));
        let ran = chain.run(SENDER, call(0, CONTRACT, U256::ZERO, GAS, &[]));
        assert!(ran.success, "{callee_code}");
        words(&ran.output)
    };
    let pushed: Vec<_> = callee_codes
        .iter()
        .map(|code| run(&statically, code))
        .collect();
    let (zero, one) = (vec![U256::ZERO], vec![U256::from(1)]);
    assert_eq!(pushed, [zero.clone(), one.clone(), zero], "by a STATICCALL");
    let pushed: Vec<_> = callee_codes
        .iter()
        .map(|code| run(&plainly, code))
        .collect();
    assert_eq!(pushed, [one.clone(), one.clone(), one], "by a CALL");
}

/// The init code CREATE2 runs in the cases below: 0x60, a PUSH1 that the code
/// ends before, which pushes 0 and stops, leaving the new contract no code.
const PUSH1_AT_THE_END: &str = "PUSH1 0x60 PUSH1 0x00 MSTORE8";

/// CREATE2 of 10 wei, salt 0, and the byte at 0 of memory as its init code.
const CREATE2_OF_THE_BYTE: &str = "PUSH1 0x00 PUSH1 0x01 PUSH1 0x00 PUSH1 0x0a CREATE2";

/// rskj's `Create2Test.testCREATE2_DuplicateContractCreation`. From RSKIP125
/// `Program.createContract` refuses a creation at an address that holds code
/// or a nonce, and the contract a first CREATE2 made holds a nonce (1) and no
/// code: a second CREATE2 of the same init code and salt makes nothing, moves
/// no value, and spends all the gas it was given, all the program had. The
/// STOP after it needs none, so the transaction succeeds.
#[test]
fn a_second_create2_at_the_address_of_one_it_made_fails_with_all_the_gas() {
    let code = compile(&format!(
        "{PUSH1_AT_THE_END} {CREATE2_OF_THE_BYTE} {CREATE2_OF_THE_BYTE} STOP"
    ));
    let mut chain = Chain::mainnet(IRIS);
    chain.put(SENDER, 0, ONE_RBTC, &[]);
    chain.put(CONTRACT, 1, U256::from(1_000), &code);
    let ran = chain.run(SENDER, call(0, CONTRACT, U256::ZERO, GAS, &[]));
    assert!(ran.success);
    assert_eq!(
        ran.gas_used, GAS,
        "the second CREATE2 spent the gas it was given"
    );
    let made = CONTRACT.create2_from_code(B256::ZERO, [0x60]);
    let account = chain.account(made).expect("the first CREATE2's contract");
    assert_eq!(
        (account.nonce, account.balance),
        (U256::from(1), U256::from(10))
    );
    assert_eq!(
        chain.balance(CONTRACT),
        U256::from(990),
        "the second moved no value"
    );
}

/// rskj's `Create2Test.testCREATE2_PreserveContractAddressBalance`: CREATE2 at
/// an address that holds a balance keeps it, and adds the value it moves.
/// `Program.createContract` carries an existing account's balance over
/// (`createAccount(contractAddress, existingAccount)`) at every height; only
/// a creation transaction waits for RSKIP174 (iris300) to.
#[test]
fn create2_keeps_the_balance_already_at_the_address_at_every_height() {
    let code = compile(&format!("{PUSH1_AT_THE_END} {CREATE2_OF_THE_BYTE}"));
    let code = [code, return_top(1)].concat();
    let made = CONTRACT.create2_from_code(B256::ZERO, [0x60]);
    for number in [WASABI, IRIS - 1, IRIS] {
        let mut chain = Chain::mainnet(number);
        chain.put(SENDER, 0, ONE_RBTC, &[]);
        chain.put(CONTRACT, 1, U256::from(1_000), &code);
        chain.put(made, 0, U256::from(100), &[]);
        let ran = chain.run(SENDER, call(0, CONTRACT, U256::ZERO, GAS, &[]));
        let pushed = U256::from_be_slice(made.as_slice());
        assert_eq!(words(&ran.output), [pushed], "#{number}");
        assert_eq!(chain.balance(made), U256::from(110), "#{number}");
    }
}
