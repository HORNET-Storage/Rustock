//! The EVM's rules as rskj's `VM`, `Program` and `TransactionExecutor` hold
//! them, where no ported test holds a rule or one side of its activation:
//! each case runs as a contract's code in a block, on each side of the height
//! rskj's `reference.conf` gives the rule.

use alloy_primitives::{Address, U256};

use super::harness::{
    call, compile, push_address, return_top, run_code, words, Chain, CONTRACT, GAS, ONE_RBTC,
    SENDER,
};

/// orchid on mainnet: RSKIP91.
const ORCHID: u64 = 729_000;
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
