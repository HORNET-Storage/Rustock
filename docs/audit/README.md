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
13 of 40 were stale spec text where acting on the recommendation would have
forked, while three findings rated LOW were real EVM divergences. The same
inversion is expected in the remainder — and the first pass found it again.

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
rules rskj's `reference.conf` schedules, the index at `5736b2c` lists 13 as
Draft (RSKIP-110, -119, -122, -134, -152, -351, -529, -535, -536, -540, -544,
-551, -552) and does not list 13 at all (RSKIP-88, -90, -97, -103, -124, -126,
-136, -137, -143, -150, -151, -156, -559). The 13 Drafts fall in the class
#149 set aside as not consensus, and the 13 unlisted were outside the audit
altogether. The index also disagrees with the RSKIPs' own files: it has
RSKIP-131, -179 and -191 as Adopted where their files say Draft, and RSKIP-59
as Accepted where its file says Rejected.

None of those 26 is among the eleven, and this pass did not read them rule by
rule. What covers them now is the chain: every block of both networks has
been re-run and matches its header, which catches a difference wherever a
real block exercises one. 17 of them are also among the rules
`every_rule_activates_where_rskjs_config_puts_it` holds to rskj's heights.
