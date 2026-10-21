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
