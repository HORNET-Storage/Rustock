# Audit re-triage (#46)

Re-checking the bulk RSKIP compliance review in archive `rustock-4796682`
(109 reports, 64 findings, 455 checklist assertions) against **deployed rskj**
rather than against the RSKIP text.

## Why the baseline matters

The audit states its own baseline:

> Baseline | RSKIP specification (spec is authoritative; rustock deviations are findings)

rustock's acceptance criterion is the opposite: **agreement with deployed rskj
mainnet**. Where the RSKIP and rskj disagree, rskj wins, and "fixing" rustock
to match the spec would *introduce* a fork.

Re-checking the 40 medium/low findings inverted the severity column in both
directions (see [audit-triage-medium-low.md](../audit-triage-medium-low.md)):
12 of 40 were stale spec text where acting on the recommendation would have
forked, while three findings rated LOW were real EVM divergences. A thirteenth
first filed as stale spec, FRCR-477, is a real EVM divergence too: rskj reads
RSKIP-50's code header. The same inversion is expected in the remainder — and
the first pass found it again.

## Verdict vocabulary

| verdict | meaning |
|---|---|
| **Divergence from rskj** | rustock differs from deployed rskj. Real. |
| **Spec is stale** | rustock matches rskj; the RSKIP text is wrong or outdated. No action. |
| **No consensus effect** | local-only getter, RPC formatting, mempool policy. |
| **Premise does not hold** | the finding describes code that is not there, or is there and correct. |
| **Closed window, unexercised** | a genuine divergence in a permanently closed activation window, proven unexercised by the whole-chain replay. |

## Where the audit stands

| scope | status |
|---|---|
| 7 high findings (6 distinct defects) | done — #43, [audit-response-2026-09.md](../audit-response-2026-09.md) |
| 15 medium + 25 low | done — [audit-triage-medium-low.md](../audit-triage-medium-low.md), #43 and #44 |
| **17 info findings** | **done — this directory** |
| **348 checklist rows marked OK** | **not started** — the remaining bulk |
| **111 RSKIPs the audit never reviewed** | **done for the 11 Adopted or Accepted** — #149, [below](#rskips-the-audit-skipped-149) |

## The 348 OK rows

The findings tables were triaged. Each report also carries a **compliance
checklist** whose rows are marked OK, and nobody has re-checked those. A row
marked OK was checked against the RSKIP text, which is not our acceptance
criterion — so an OK row against a stale RSKIP can hide a divergence from rskj,
which is the direction that matters.

Order of work is by **blast radius**, not by the audit's severity column:

1. Anything transaction-callable on the Bridge.
2. EVM opcode availability and gas, per activation.
3. Block/header validity rules.
4. Local-only getters last — wrong RPC answers cannot fork.

## Cluster files

| file | findings |
|---|---|
| [evm.md](evm.md) | FRCR-279, -281, -577; RSKIP-131, -191 |
| [storage-state.md](storage-state.md) | FRCR-373, -375, -377, -379; RSKIP-144 |
| [rewards-fees.md](rewards-fees.md) | FRCR-876 |
| [bridge.md](bridge.md) | FRCR-73; RSKIP-179, -502 |
| [addresses-accounts.md](addresses-accounts.md) | FRCR-774 |
| [misc.md](misc.md) | FRCR-173, -174, -175, -176, -177, -374, -576; RSKIP-0, -59 |
| [mining-consensus.md](mining-consensus.md) | RSKIP-46, -157 |
| [standards.md](standards.md) | RSKIP-148, -159 |

## Result of the info pass

17 findings. **One was not info.**

| verdict | count |
|---|---:|
| Spec is stale / observation only | 9 |
| Premise does not hold | 3 |
| No consensus effect | 3 |
| Absent from rustock *and* from mainnet consensus | 1 |
| **Divergence from rskj** | **1 — FRCR-279** |

[FRCR-279](evm.md#frcr-279) was filed INFO as "no distinct RSKIP169 activation
gate". It is a real EVM divergence across a ~1.22M-block window, in a
permanently closed range that the whole-chain replay proves unexercised. The
severity column was wrong by three levels. It is now fixed, with RSKIP-131
below.

Archive: `/root/doc/rustock-4796682.zip`.

## RSKIPs the audit skipped (#149)

The audit indexed the 220 RSKIPs of the RSKIPs repository's index at
`5736b2c` and reviewed 109. #149 sorted the 111 it skipped by their status
in that index (80 Draft, 6 Rejected, 14 without a spec file, 4 Accepted,
7 Adopted) and named the 11 Adopted or Accepted as the ones to check. Each
was read in rskj as deployed, `VETIVER-9.0.4` (`master` at `730c235ad`
changes none of the code involved), and in rustock:

| RSKIP | title | file | verdict |
|---|---|---|---|
| 131 | CREATE2 after SUICIDE in the same block | [evm.md](evm.md#rskip-131--create2-after-suicide-in-the-same-block) | **divergence from rskj — fixed** |
| 191 | DUPN, SWAPN and TXINDEX removed | [evm.md](evm.md#rskip-191--dupn-swapn-and-txindex-removed) | **divergence, closed window — fixed** |
| 179 | BTC-RSK timestamp linking | [bridge.md](bridge.md#rskip-179--btc-rsk-timestamp-linking) | matches rskj |
| 502 | PowPeg and Union Bridge integration | [bridge.md](bridge.md#rskip-502--powpeg-and-union-bridge-integration) | **divergence, live on mainnet — fixed** |
| 46 | mining fees in the header | [mining-consensus.md](mining-consensus.md) | matches rskj |
| 157 | cumulative difficulty in JSON-RPC | [mining-consensus.md](mining-consensus.md) | no consensus effect; matches rskj |
| 148 | ERC1820 registry | [standards.md](standards.md) | premise does not hold |
| 159 | minimal proxy contract | [standards.md](standards.md) | premise does not hold |
| 144 | parallel transaction execution | [storage-state.md](storage-state.md#rskip-144--parallel-transaction-execution) | matches rskj; live on testnet |
| 0 | RSKIP purpose and guidelines | [misc.md](misc.md) | no consensus effect |
| 59 | child contracts | [misc.md](misc.md) | absent from rskj and rustock |

Three of the eleven were real divergences, and #149's spot check had the
first one backwards: the test it cited as covering RSKIP-131 asserted the
creation rskj refuses. The one that mattered most was RSKIP-502: an
upgrade-order gate made seven Bridge methods callable on mainnet from
#8,804,200, where rskj has none of them, so one transaction calling one would
have split rustock from the chain. RSKIP-144 is not the closed window #149
took it for: testnet runs it.

### What sorting by status leaves out

A rule is consensus when rskj runs it, whatever its RSKIP's status. Of the 97
RSKIP rules rskj's `reference.conf` schedules (its 98th rule,
`areBridgeTxsPaid`, has no RSKIP, and rustock schedules it at rskj's heights),
the index at `5736b2c`, which #149 sorted by, lists 13 as Draft (RSKIP-110,
-119, -122, -134, -152, -351, -529, -535, -536, -540, -544, -551, -552) and
does not list 13 at all (RSKIP-88, -90, -97, -103, -124, -126, -136, -137,
-143, -150, -151, -156, -559). The 13 Drafts fall in the class #149 set aside
as not consensus, and the 13 unlisted were outside the audit altogether. The
index has moved since: as it stands now (last changed 16 October), it has seven
of those Drafts as Adopted (RSKIP-119, -152, -536, -540, -544, -551, -552),
three as Adopted (Testnet) (RSKIP-351, -529, -535), and RSKIP-559 as Draft. The
index at `5736b2c` also disagrees with the RSKIPs' own files: it has RSKIP-131,
-179 and -191 as Adopted where their files say Draft, and RSKIP-59 as Accepted
where its file says Rejected.

None of those 26 is among the eleven, and the chain can't settle them: every
block of both networks has been re-run and matches its header, which shows a
rule matches wherever a real block exercised it, but says nothing of a case no
block reached, or of a block rskj refuses, since the chain holds none. So each
was read where rskj reads it, at `VETIVER-9.0.4`, against rustock. Five
differed:

| RSKIP | rskj | rustock did | fix, and its test |
|---|---|---|---|
| 90 | Before orchid (mainnet #729,000), a precompile that returns leaves the caller's return data as it was (`Program.executePrecompiled`). | Made the output the return data. | `RskHandler::with_rskip90`; `test_precompile_calls_and_the_return_data_by_height` |
| 103 | Before orchid060 (mainnet #729,000 to #1,052,699), STATICCALL takes a value as CALL does (`VM.calculateCallValue`): seven arguments, `VT_CALL` and the stipend for a value, which moves to a callee that runs static. | Took six arguments and no value. | `rsk_static_call_with_value`; `test_pre_rskip103_staticcall_takes_a_value` |
| 134 | `increaseLockingCap` fails the call where the new cap times its multiplier passes a `long` (`Coin.multiply`). | Saturated the product and went on. | `governance.rs`; `a_cap_whose_limit_passes_a_long_fails_the_increase` |
| 136 | Before bahamas (mainnet #3,397, or wherever `rskip136` is moved), a transaction's own call to any precompile is held to its limit by the precompile's price alone, and pays that price on top of the intrinsic cost, past the limit if it comes to that (`TransactionExecutor.call`). | Did so for the Bridge only. | the executor's direct-call path; `before_rskip136_a_direct_precompile_call_is_held_to_its_limit_by_its_price_alone`, and the same for a native contract |
| 150 | Before twoToThree (mainnet #2,018,000, testnet #504,000), a call's stipend is added to the gas it asks for as a plain `long` (`VM.getMessageCall`): at 2^63 - 2,300 or more the sum goes below zero, and the calling frame fails with all its gas. | Saturated the sum. | `rsk_callee_gas`; `test_pre_rskip150_stipend_sum_past_a_long_fails_the_caller` |

Four of the five sit in windows the chain has passed, where no block reached
the case, and RSKIP-134's needs a cap no authorizer has asked for.

RSKIP-136 runs through the path for the calls rskj makes outside its VM, which
also takes the federation's free Bridge transactions. Three more differences
in that path are fixed with it:

- It held a transaction to none of `TransactionExecutor.init`: its sender's
  next nonce, a limit that pays its intrinsic cost, a balance that covers its
  value and its whole limit. revm holds every other transaction to these.
  `test_bridge_tx_gas0_fails_for_unknown_sender` asserted a block that `init`
  refuses, which `TransactionListExecutor` stops at; it now asserts the
  refusal. A free Bridge transaction meets this on rskj's regtest at any
  height, where the federators' Bridge transactions stay free.
- It priced a byte of data at 68 gas where RSKIP400 makes it 16.
- Where a block runs as its sublists, it paid each fee to REMASC at once, and
  REMASC was paid it again with the block's other fees.

The last two, and RSKIP-136's own difference, can't happen on mainnet or
testnet: mainnet's window is its first 3,397 blocks, which rustock replays,
and testnet schedules bahamas at 0. They can on a chain that moves `rskip136`.

The other 21 match:

| RSKIP | what rskj gates on it |
|---|---|
| 88 | the Bridge refusing calls from contracts, and how it ends a call it can't read |
| 97 | the difficulty's ten-minute rule and its minimum |
| 110 | the fork-detection data in a merged-mining coinbase |
| 119 | the BlockHeader precompile |
| 122 | the Bridge's `getBtcTransactionConfirmations`, its price included |
| 124 | the Bridge's `receiveHeaders`: its price, who may call it, a header's size |
| 126 | the state root a header carries, and how a block's roots are checked |
| 137 | the altBN128 precompiles, and their answer to a point off the curve before RSKIP197 |
| 143 | the Bridge's coinbase-information methods |
| 151 | SELFBALANCE |
| 152 | CHAINID |
| 156 | the difficulty's bound divisor, 400 from it except on regtest |
| 351 | a block's sublist gas limits and their header field |
| 529 | the Union Bridge's event methods |
| 535 | the header's version 2, and the Union Bridge's base event it carries |
| 536 | the BlockHeader precompile's `getCumulativeWork` and `getDifficultyWithUncles` |
| 540 | the Bridge's `getEstimatedFeesForPegOutAmount` |
| 544 | refusing new code that starts with 0xEF |
| 551 | where a rejected peg-in stops being marked as processed |
| 552 | BLAKE2F's price for a call with no data |
| 559 | sorting the pegouts; no network schedules it, and `tbd1000`, its upgrade, is accepted and reported as changing nothing |

18 of the 26 are also among the rules
`every_rule_activates_where_rskjs_config_puts_it` holds to rskj's heights.
