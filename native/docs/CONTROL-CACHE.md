# General computer/browser cache reuse and editing fixes

These changes apply to the shared AI control loop, not to any particular application. No Notepad-specific production logic is used.

## Root cause

The diagnostic report supplied for a 69-round task showed 41 requests with changed earlier context. Every one reused only 1,002 tokens. Those requests accounted for 92.3% of model time; median request time was 15.54 seconds after a rewrite versus 1.43 seconds on unchanged history. The installed chat context was 32,768 tokens, independently of the server’s larger supported maximum. Repeated large snapshots filled the client budget; dropping the oldest exchange just far enough below that budget caused another rewrite after almost every new observation.

## New shared context policy

- Append completed tool exchanges unchanged between checkpoints. There are no extra model calls for summarization.
- Use a control-context soft ceiling of 48,000 estimated bytes, or the smaller existing context budget. This is not an exact tokenizer count.
- At a checkpoint, reduce history toward 55% of that ceiling, keeping the latest two complete tool exchanges and the original task. A bounded, deterministic checkpoint records up to twelve historical tool-reported mutations/launch requests. It is marked historical and untrusted, not proof of current state. Fresh snapshot and identity checks remain enforced.
- Keep up to three control screenshots between checkpoints. Remove older screenshots together at a checkpoint, retaining the newest; this trades bounded additional vision context for fewer edits to earlier prompt messages. Small configured contexts can force earlier checkpoints.
- Native control observations omit empty fields and non-screenshot geometry. Fingerprints still retain full bounds internally for stale-target checks. Document excerpts are 1,600 characters with explicit truncation/paging, and control results are capped at 12 KB rather than 40 KB. Large interfaces may need paged inspection.
- Checkpoints still invalidate some cached prompt processing. The goal is fewer invalidations with headroom between them, not unlimited context or a promise of zero cache misses. Backend eviction, templates, model reloads and other traffic can still affect reuse.

## Local-server cache probe

A synthetic replay compared rolling removal with checkpoint compaction on the configured local server. Both used the same generated observations and a 48 KB history threshold, 16 requests per policy; no computer actions were executed. The new policy was exercised through the production Rust context implementation.

| Metric | Rolling removal | Checkpoint compaction |
|---|---:|---:|
| Earlier-context rewrites | 9 | 2 |
| Reported cached/input tokens | 23.2% | 72.1% |
| Median request time | 4.98 s | 1.89 s |
| Total input tokens across requests | 107,623 | 84,584 |

This is a small sequential server-level probe, not a replay of the user’s Notepad session or a guaranteed application speedup. The rolling run also had timing outliers, so median latency and cache counts are more useful than total duration. The probe script reads the configured account-protected API key in memory and exports numeric results only.

## General text input and focus

Computer `type` now requires an explicit `text_mode`: `insert` types at the caret/replaces an existing selection; `replace_selection` requires a nonempty selection and preserves surrounding text; `replace_all` explicitly overwrites the whole field/document. Missing mode is rejected before changing text. Browser `type` retains its existing whole-field behavior.

Native Win32 edit controls use targeted selection replacement; other editors use verified focus before keyboard input. Key actions can target a specific observed editor, and focused controls must belong to the selected window. Ctrl+H and Delete are supported along with Ctrl/Shift navigation. Unsupported double-click/drag is stated explicitly in the tool description. Existing permission, password, protected-window, stale-reference and emergency-stop boundaries remain in force.

## Test with your own server

Start a new AI conversation after installing, keeping the same model, context limit and action limit for comparisons. Use multiple applications and longer tasks, not just text editing. Copy diagnostics from a slow run. Look for multiple successive steps retaining most input tokens, with context-change flags at occasional checkpoints rather than nearly every round. A first request, checkpoint, model reload or server cache eviction can still have little reuse.

For editing, use a disposable document and ask for three word replacements while preserving all surrounding text. Also test insertion, selection replacement, Ctrl+H, and switching focus between Find and the document. Do not use valuable unsaved documents for regression testing.

## Follow-up task boundaries

A follow-up regression was found in checkpoint and hard-budget removal: removing everything between adjacent tool-call messages could also remove the previous answer and the latest human instruction when recalled tool history preceded a new turn. The model could then continue an earlier task. Removal now stops after the tool replies and their generated screenshot observations; conversational answers, follow-up requests and user attachments are retained. Checkpoint notes explicitly identify the latest human request as the active task.

Regression coverage forces compaction across recalled history into the current turn for append, save and save-to-folder requests, checks that the instruction and preceding answer survive in order, and verifies tool-call/reply pairing. A separate hard-budget boundary test preserves an image-bearing follow-up. This is a shared conversation fix, not a Notepad-specific workaround.

The subsequent three user-provided traces reported 70.8%, 66.8% and 65.5% cached/input tokens, with median model request times of 2.78 s, 3.19 s and 3.27 s respectively. This confirms substantial reuse on those runs, while the task-boundary regression made follow-up correctness unacceptable. These are different workflows from the synthetic probe, so their timings are not a controlled before/after comparison. The duplicated third trace is counted once. The diagnostic export contains no prompt text, so it cannot independently prove which request the server received or verify the claimed final document state.

## Native input and modal reliability

A later test retained follow-up intent but exposed native interaction problems. Resolving a numbered element by the first matching automation ID could choose a different control, since Windows dialogs reuse IDs such as `1`. Fresh lookup now requires a unique match of identity, type, observed name and bounds. Duplicate IDs are omitted from exported observations so the model uses the exact numbered element; supplying both a number and ID can disambiguate the current snapshot. Missing or conflicting matches remain rejected.

When a selected window is disabled by its same-process owned modal, inspect, screenshots and actions now resolve to that modal together. Closing it returns subsequent observations to the parent. Other-process dialogs are rejected and existing password, protected-process and stale-target checks remain. Window titles are refreshed rather than replayed from the initial window list.

Keyboard fallback text is sent in small paced Unicode batches, with foreground/focus checks between batches, explicit Enter for LF/CRLF, and a settling interval before restoring VectorDash. Native edit/value-pattern paths remain available. Partial input is not automatically retried. This reduces burst/race exposure; successful SendInput is not independent verification of final text.

`control_input_smoke.py` uses disposable windows to verify modal screenshots and correct numbered selection of the second of two identically named/identified buttons. Its optional keyboard fixture checks long Unicode, surrogate pairs and multiline text, but explicitly reports SKIP if Windows denies foreground transfer; `--require-keyboard` makes that a failure. A skipped fixture must not be reported as proof that real Notepad typing passes.

On the final optimized build, foreground transfer was available and all three exact-content keyboard runs passed, including long Unicode, surrogate pairs, LF and CRLF. The modal screenshot/duplicate-ID test, existing native/browser fixture, and follow-ups under both permission choices passed as well. All 329 Rust tests passed (19 optional fixtures ignored). These disposable fixtures verify the native primitives; they do not establish that a real-model Notepad task or save succeeded.

## Exact replacement and Save As hardening

Computer `type` now supports `text_mode: replace_text` with `find_text` and replacement `text`. It selects the first case-sensitive literal match in the intended editor and reuses selection replacement. Native Edit/RichEdit controls use UTF-16 selection offsets; other editors require UIA TextPattern FindText/Select. Missing matches or unsupported providers fail without changing text. The tool description prefers exact selection to counting cursor movements. This is a generic editing primitive, not a Notepad macro.

Native whole-field replacement now uses selection plus EM_REPLACESEL instead of WM_SETTEXT, preserving normal edit notifications and undo semantics. The real Windows Save As fixture checks the filename actually returned by the dialog, rather than just its visible edit value; it leaves an existing fixture file intact and never writes the selected output. This fixture also passed with the previous build, so it did not reproduce the reported Notepad wrong-filename conflict. The notification change is hardening, not a confirmed root-cause fix for that report. Do not attribute the conflict to OneDrive or Desktop redirection without evidence.

Native regression checks verify dog→cat→flying cow replacements with a Unicode prefix and unchanged surrounding text, plus missing-match rejection. Real-model Notepad replacement planning and the exact jokes2.txt save conflict still require retesting.
