# Automatic indexing and action responsiveness audit

Unreleased follow-up on PR #16 after 0.3.25-beta.4.

## Observed behavior

Recent logs showed normal targeted updates checking one or a few files, and one 508-path notification batch checking 547 entries. A created/renamed folder legitimately includes its descendants. These are not complete-drive scans. Logs also showed an avoidable queued recovery scan checking about 1.13 million files after a manual update. Independently, refreshing the displayed database totals queried the index broadly; those SQL counts did not re-read files, but repeating them during bursts wasted resources.

## Changes

- A successful full/manual scan acknowledges only notification work queued before it started. Repeated writes and recovery signals queued during that scan survive. The old startup-recovery flag is cleared when already covered. Failed/cancelled scans do not acknowledge queued work.
- Known change paths stay durable and targeted beyond the former 4,096-path threshold, drained in batches of at most 4,096. The callback's busy-channel spill retains up to 32,768 exact paths. SQL queue reads use a queued-time index and generation acknowledgements use primary-key lookups/prepared statements.
- Memory remains bounded if the durable queue cannot be written. Exceptional catch-up reasons are visible in Logs. Extreme callback spill saturation checks only affected configured sources rather than unrelated sources; unknown lost-event information can still require broader recovery.
- Targeted runs resume only Full jobs and deferred Quick names within the notified file/folder scope. Unrelated unfinished jobs are left for their own notification or a full update. Directory metadata-only events remain ignored; create/rename folder events still cover that subtree.
- Display statistics use one coalesced worker, with at least five seconds between database-wide recounts during bursts. Counts may briefly lag; searchable index updates are not delayed by the statistics cooldown.
- Logs include the actual targeted scope and actual inaccessible path, rather than attributing every vanished temporary file to an unavailable containing drive.
- Enter/click activation cancels the stale semantic query, snapshots the selected path and dispatches Windows opening on a separate thread. The compact popup closes immediately. Failures are reported and restore a hidden popup. COM initialization and a completed shell handoff keep shortcuts/packaged-app activation safe after the worker exits. Windows retains normal application-startup time; there is no embedding dependency.

## Necessary broader scans

An initial catch-up without continuous journal coverage, a journal gap, or a provider notification-loss signal may require checking more files because exact changed paths are unknown. This is a reliability fallback, not the response to an ordinary file-change notification. Folder creation/rename notifications cover the named subtree. Explicit Update index and configured scheduled updates intentionally check their configured sources. Existing idle/game/power rules still gate automatic work.

## Validation

173 automated tests passed (10 existing opt-in tests not run), and build/formatting checks passed. The screenshot dialog regression fixtures were also rechecked with the combined build.

The tests cover real Windows notification delivery after a manual scan: the old recovery is acknowledged, the next created file produces a one-file targeted update, and an unchanged sibling's scan epoch stays unchanged. Ten thousand known paths drain in three bounded targeted batches without duplicates or a global recovery. Other tests preserve newer writes/recovery generations, leave unrelated Full/Quick jobs untouched, bound memory during queue failure, and coalesce a 50-request statistics burst.

A blocked local embedding-server fixture verifies independent selected-action dispatch. Additional real Windows checks launched a harmless, window-free fixture executable and its shortcut before releasing the embedding response (test durations 0.11 s and 0.82 s respectively). These are handoff regression checks, not general application-startup benchmarks.

Shell handoff reference: [Microsoft SHELLEXECUTEINFO documentation](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shellexecuteinfow). No external embeddings server or active user index was changed for these tests.
