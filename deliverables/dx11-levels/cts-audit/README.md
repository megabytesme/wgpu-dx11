# CTS skip audit

This audit extracts every selector summary with one or more runtime skips from the retained FL9_1 CTS run and verifies the observed totals against its machine-readable receipt. It intentionally leaves every runtime skip as mandatory review: the saved summary log does not include subcase IDs or skip reasons, so the log cannot justify waiving any case.

The pinned `test.lst` contains 2,883 noncomment rows. The `xtask/src/cts.rs` list parser and exact backend membership rule skip the 1,197 rows whose `fails-if` conditions include `dx11`; the remaining 1,686 match the transcript summary count. `excluded-selectors.csv` preserves each source line, conditions, selector and a conservative capability-family triage. These are selectors, not case-level dispositions, and all require review.

## Reproduce

Run from the roadmap worktree with Python 3:

```powershell
python deliverables\dx11-levels\cts-audit\classify_cts.py `
  --tests-json C:\dev\personal\wgpu-dx11-FL91-cert-Nov21\deliverables\dx11-levels\FL9_1\tests.json `
  --transcript E:\Temp\wgpu_dx11_curated_cts_after_texture_ok_gate_20261117.log `
  --test-list C:\dev\personal\wgpu-dx11-FL91-cert-Nov21\cts_runner\test.lst `
  --cts-revision C:\dev\personal\wgpu-dx11-FL91-cert-Nov21\cts_runner\revision.txt `
  --out deliverables\dx11-levels\cts-audit
```

The script checks transcript summary count, pass/warning/skip/fail totals, test-list SHA-256, and CTS revision. Outputs are `runtime-skips.csv`, `excluded-selectors.csv`, and `summary.json`. The list partition and exclusion count are checked against the receipt and transcript.

## Findings and limits

- The transcript has 1,686 selector summaries. All 17,962 pass occurrences, 4,720 skip occurrences, and zero warning/failure occurrences match the receipt.
- The 4,720 skips occur in 335 selector summaries. The compact `runtime-skips.csv` records selectors and skipped counts. The follow-up below recovers case IDs and reasons from the detailed captures.
- The test list SHA-256 matches the receipt (`4c0b42ac45af18f805713b79cba55456d29c304a4ef02094c2cb90aa43cdc103`), and `revision.txt` matches CTS revision `3fdd32363410f2bd4e3e2f046980967e6abc27e6`.
- The retained summary log SHA-256 is `9f447e888b7b2c2fa9cd6afa7227a2e9eed25846964103d0606bb40c3a175e6a`, while the receipt's `full_transcript_sha256` is `f185360a60dbb80b8759e1d7b9cc2cd15b83e45b3c9347bb47bcf7ab56ba4ce1`. This is a summary-log artifact, not byte-identical to the full transcript hash in the receipt; count agreement does not establish that the retained log is the exact hashed original.
- Exclusion matching reproduces `xtask/src/cts.rs` lines 233–258 and 476–485: `fails-if` is split on commas and matched against the exact backend name. The source-list inventory does not cover selectors outside that curated list.
- Capability families are heuristic triage only. Expand mixed selectors to individual cases: unsupported positive operations may be inapplicable, while negative validation and supported mandatory behavior can still be required. A sound disposition needs case-level outcomes, reasons and capability evidence. Until then all remain unresolved.

## Detailed capture follow-up

The FL9_1 lane retained two detailed captures. Both match the full-transcript SHA-256 values in the committed FL9_1 receipts. Together they contain 9,440 detail blocks for 4,720 exact case IDs, with each ID printed twice. The committed case inventory matches that set exactly. The earlier compact summary remains a separate artifact.

Run the follow-up from this roadmap worktree with Python 3.11 or later.

```powershell
python deliverables\dx11-levels\cts-audit\review_case_skips.py `
  --lane C:\dev\personal\wgpu-dx11-FL91-cert-Nov21 `
  --logs "$env:TEMP\wgpu-fl91-cert-Nov21" `
  --out deliverables\dx11-levels\cts-audit
```

The tool reads immutable Git blobs for the lane receipt, CTS sources and FL9_1 capability fixture. [Case review](case-review.csv) records the archived log line, skip reason, pinned condition and remaining validation for all 86 previously unresolved runtime cases. [Capture and count receipts](case-review-summary.json)

| Source condition | Exact cases | QA disposition |
| --- | ---: | --- |
| Generated bare vertex output has no position builtin | 49 | The generator condition explains the skip. Keep legal struct cases in the regression set. |
| Sparse attachment arrays exceed the reported one-target limit | 21 | The old fixture skips setups beyond the limit. Keep explicit above-limit rejection in scope. |
| Required `rg11b10ufloat` attachment unavailable | 4 | The cited FL9_1 application format row records the missing native attachment use. Keep unsupported-format validation in scope. |
| Fixture needs a 4× source for a format whose FL9_1 application table lacks 4× and resolve | 9 | Retain native per-format checks and unsupported-combination validation separately. |
| Headless runner cannot create canvases | 2 | Requires browser execution with this backend to establish canvas validation. |
| `unrestricted_pointer_parameters` language extension unavailable | 1 | Track extension reporting and negative language validation separately. |

This is an audit of historical case outcomes against the pinned sources. It grants zero waivers and does not replace an exact-current-candidate rerun. [WGSL language extensions](https://www.w3.org/TR/WGSL/#language-extensions)
