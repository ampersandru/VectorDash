# Computer and browser control test prompts

Use Qev **Execute** with vision enabled for the first pass. Repeat selected prompts with **Off** using the same main model to compare total time, action count, Qev calls/executions, fallbacks, and cache reuse. Export diagnostics after each response. A fast final answer is not success unless the requested application state changed.

Start each numbered scenario in a new chat; send follow-ups in that same chat. Do not manually help unless it gets stuck; record where help was needed. Stop repeated actions if there is no progress.

## Spotify: library and follow-up

1. Open Spotify and play my Release Radar playlist. Use its existing library entry if available. Verify the playlist and active playback once, then stop.
2. Now play ZHU's latest single. Use Spotify's Latest Release label to identify it. Once the matching release and active playback are verified, stop; don't reopen the artist page to verify the same facts again.
3. Now play John Summit's latest single. Tell me the track, release, and playback device. Don't switch playback devices unless I ask.

## Spotify: navigation and dropdowns

1. Open Spotify, search for Kaskade, then replace the search with John Summit. Dismiss suggestions, open John Summit's artist page, select Singles and EPs, and play the newest listed release. Verify once and stop.
2. Pause it, then return to my Release Radar playlist and start playing it.

## Spotify: reading and scrolling

Open Spotify and open John Summit's artist page. Scroll the main content pane to Discography, select Singles and EPs, and list the first three releases shown. Do not play anything or alter my library. If scrolling doesn't move the page, change strategy instead of repeating the same input.

## Notepad: text and follow-up

1. Open a NEW blank Notepad tab, preserving all existing tabs. Type this two-line text exactly, then replace every dog with cat and every cat with flying cow:
The dog found another dog.
The dog says hello!
2. Append a blank line and a new joke. Preserve the existing two lines and verify the appended text appears only once.
3. Save this new document as vectordash-control-test-01.txt on my actual Desktop using the observed Desktop location. If the file exists, stop rather than overwrite it. Verify the filename and unmodified state.

## Restaurant research: browser control

Test assumptions: two people; $50 per person for food before tax/tip. Change these if needed. October 30, 2026 is Friday. Use the exact date so relative dates do not drift.

Use browser control to research Italian restaurants near Bernal Heights, San Francisco, for two people on Friday, October 30, 2026, around 7:00 PM Pacific (6:30-7:30 is acceptable). Aim for about $50 per person for food before tax and tip. Require a reservation without a credit card and free cancellation. Compare up to three options. Check live table availability, menu prices, credit-card requirements, and cancellation terms on the restaurant or booking provider's site. Cite the pages supporting each finding and clearly label anything you cannot verify. Do not infer free cancellation or no-card booking from silence. Do not submit a reservation or enter payment information. Stop with your best verified option and any missing information.

Follow-up:
Take the best verified option to the final booking review for Andrew Nguyen, keeping the same date, party size, budget, and cancellation requirements. Ask me for any missing contact details instead of inventing them. Do not accept fees, enter payment information, opt into marketing, or submit the reservation. Stop before final confirmation and summarize the restaurant, time, party size, name, and exact cancellation terms.

## What to report

- Prompt, decision mode, main model, and elapsed time.
- Whether the task actually completed and any manual intervention.
- Exported step diagnostics, including Qev routing, scores, executions, fallbacks, observation offsets, scrolling movement, and prompt/cache counts.
- Repeated clicks, dropdowns left open, scrolling the sidebar, duplicate text, false verification errors, or needless checks after completion.

## Restaurant regression checks

Repeat the same restaurant prompt in a fresh chat for Execute and Off, with the same main model and action budget. Keep the original criteria unchanged so the comparison is useful.

Watch for these changes:

- Research should retain previously observed source excerpts after checkpoints rather than repeatedly rediscover candidates. These excerpts remain historical, untrusted evidence, not confirmed current policies.
- Three visits to the same URL produce a warning; after five visits, further identical navigation is blocked for that request. Different booking dates remain distinct URLs. This does not prevent switching to another source.
- Missing controls should prompt inspection of page_state and embedded_frames. Frame discovery alone does not mean its contents were read; supported HTTP(S) widget links can be opened explicitly. Inline frame interaction and PDF menu extraction are not added by this change.
- If control reaches its budget, a VectorDash notice appears before the final summary. The final diagnostic row should have forced_final=true, status=limit, and stop_reason=round_limit (or the applicable time/retry/permission reason).
- Diagnostics now include allowlisted page_state and an embedded_frames count, without recording the page URL or contents.

Success means the booking requirements are supported by observed evidence, or remaining unknowns are clearly reported without a booking. A shorter answer or a claimed successful search alone is insufficient.

## Structured evidence and grouped-loop regression

Use the same original restaurant prompt for the next comparison. Keep the model, action budget, party size, date and price constraints unchanged.

- Source records retain complete observed URLs. Oversized records are shortened by dropping individual quotations or omitted in full, with omission markers; serialized records are never cut mid-URL.
- Cancellation and booking-payment quotations take priority over generic navigation/menu text. Conflicting quotations remain historical evidence, not automatically verified facts. Accepted payment methods have their own category and do not establish a card-hold requirement.
- Tracking parameters and query ordering share an exact-URL counter. Source-page variants share a second counter; search query/filter variants share a broader search-page counter. Dates, party sizes, venue IDs and pagination remain distinct. Limits are 5 exact, 8 source-family and 12 search-family visits per request.
- Diagnostics include navigation_scope (exact/source/search) and navigation_repeats. Look for warnings and navigation_loop errors before 128 rounds of repeated discovery.
- The final review instruction asks the model to reconcile candidates, exclusions and conflicts; preserve fee evidence; check requested date/time/party/price constraints; and distinguish a round limit from permission denial. This is model guidance, not a deterministic fact checker.
- Report any lost policy evidence, partial citation URL, repeated candidate rediscovery, or time outside 6:30-7:30 PM described as a match. Do not submit a reservation.

## Candidate workflow regression

For English booking research/comparison requests, the browser workflow now asks the model to annotate ordinary calls with candidate, stage and outcome metadata. This is request-local memory, and model-reported outcomes are not independent verification.

- Broad discovery is limited to ten navigation attempts before selecting candidates. The default comparison list has three active candidates; the model may set the requested candidate_limit in the first annotation. Excluded candidates remain in the records while a replacement can be selected, up to twelve total retained candidates.
- Each details/pricing/availability/card_policy/cancellation stage has five main-model browser attempts. An unresolved stage can be closed once using unknown/blocked metadata on a snapshot, then the model must move to a different missing check or summarize.
- Observed profile titles and booking links can associate an anonymous booking flow with a candidate by its observed provider venue ID. Multiple conflicting widget IDs remain unassigned. A model-supplied candidate name alone does not prove venue identity.
- Qev can match a refreshed hyperlink label using the same unique complete destination on the same original page. Different destinations, duplicate destinations, changed pages, and changed button labels do not receive this recovery.
- The checkpoint-note allowance is reduced from 12 KB to 6 KB. Candidate names/statuses are retained with explicit optional-field omission, alongside source evidence. Compare cache reuse and uncached input; do not assume improvement from elapsed time alone.
- New diagnostic fields are research_candidate_count, research_stage, research_stage_attempts, research_bound and decision_href_recovered. Candidate names and source URLs remain outside the diagnostic export.

Repeat the unchanged restaurant prompt with Execute and Off in fresh chats. Report whether candidates survive into the final comparison, booking flows are attributed correctly, stage limits cause useful progress rather than repeated errors, and overall rounds/uncached input decrease. No reservation should be submitted.


## Windows activation and dialog recovery regression

Repeat the seven-turn Documents/Notepad/Task Manager/Archive workflow in a fresh chat with Computer enabled and Qev Execute. Use a new test directory, and leave unrelated files untouched. No restaurant test or Off comparison is required for this change.

- Explorer items should be opened with the explicit observed-element `click` / `activate: true` option. Ordinary selection is still distinct from opening. Activation only supports observed list/tree items, checks target identity, selection and focus, and returns a fresh observation; dispatch alone does not prove the requested destination opened.
- Opening an owned dialog or returning from a closed dialog should preserve the application context. One read-only inspection retry can recover transient UIA/layout changes. Input is never replayed as part of that recovery. A directly addressed closed dialog may resolve back to its checked same-process root owner.
- Diagnostics add `modal_transition` (`opened`, `closed`, `none`), `observation_recovered`, and action method `activate_enter`. They contain no paths or document contents.
- Exact named Qev plan goals omit unrelated candidate controls while retaining duplicate exact names. The confidence threshold and existing consent/freshness checks are unchanged.
- The model is instructed to reuse returned fresh observations and stop rechecking completed outcomes when nothing relevant changed. This is guidance, not a guarantee of fewer rounds; compare round counts and main-model time against the prior exports.
- Check that opening Archive changes the actual location, the backup remains unchanged, Cancel creates no file, and the final selected document is the original. Report wrong-target input, repeated actions after a dialog transition, or unsupported-key errors.


## One-file control debug transcript

In AI settings, under Computer and browser use, enable **Save computer/browser debug transcript** and save settings. It is off by default. Run the normal multi-turn test in one chat, then use **Export debug transcript...** below the AI diagnostics or in the same settings section. Share the resulting `.jsonl` file instead of separately copying answers and diagnostics.

The automatically saved file is in the app data directory's `control-debug` folder. It records the submitted user prompts, model response text and requested tool plans, actual tool arguments/results, numeric per-round/Qev diagnostics, and completion/error/cancellation state, correlated by job and tool-step numbers. New Conversation starts a new file. Disabling the option stops future request recording; existing files are retained. A request already started with debugging enabled keeps recording through completion. It does not record before the option was enabled, and does not include full system prompts or repeated historical model context.

This opt-in transcript can contain private file paths and screen/page text. API settings and image payloads are excluded; very long values/arrays are marked as truncated. Nothing is uploaded automatically. Normal logs and Copy diagnostics remain numeric-only. If a run crashes or is terminated, the flushed partial file can end before a response record; the last tool-start/result records identify the last recorded action. Avoid starting a new conversation until you export the current file, or retrieve older files with Open debug folder.


## Grounded control and local-Qev regression

Repeat the seven-turn Windows test with the same local chat model and Qev Execute, in a new test directory. Export the one-file debug transcript.

- Activation now requires the exact observed `element_name` alongside a numbered element when needed. A number/name mismatch is rejected. Ordinary list/tree clicks select; explicit activation selects, focuses and sends Enter only after foreground checks succeed. Names duplicated within a native observation retain their geometry so Qev can refresh the numbered target without treating two distinct buttons as one. Moved, mismatched or still-ambiguous controls remain rejected.
- Typing with an omitted locator can use only a single focused editable control from a fresh observation of the same window. Password, disabled, offscreen, stale, cross-window and ambiguous targets remain rejected. F2 and Alt+Left/Right/Up are supported; other Alt/system shortcuts are not.
- When a disabled application's last-popup hint is unusable, dialog discovery can select one enabled visible same-process window with the same proven root owner. It does not choose an arbitrary window merely because the process matches.
- Control response text is buffered until that model round completes. A cancelled partial round is not presented as a completed sequence. A tool-less draft for an English input request with no recorded input, or a long sequence of future-action narration, can receive one corrective turn. Repeated suspicious drafts are withheld with an incomplete notice. Already-satisfied states and honest limitations can be reported without forcing changes. This is a bounded sanity check, not proof that every part of an arbitrary task was completed.
- Debug files now include `tool_exchange`: the calls and final parsed replies actually received by the main model, including Qev/validation/observation errors that occurred before the native helper. Diagnostics include `completion_guard_reason`; a corrective round has status `completion_retry`.

Validation used a disposable Win32 fixture with the user's local LLM:instruct and managed Qev vision: Alpha/Apply, Beta/Apply, switch Off, second duplicate Save button, then Final verified chain/Apply. Fixture state was independently read after each phase. The final debug build completed in 13 rounds, with 9 Qev calls, 5 Qev-dispatched actions and no recorded tool errors. Counts vary between runs; do not infer universal speed gains. Separate mock-model tests cover withholding fabricated success, one bounded correction, and refusing a repeated unsupported completion. Direct physical-input smoke checks explicitly report a skip if Windows denies foreground input.


The installed-browser localhost test also used the real local chat model and managed Qev: open a disposable page, type Alpha/Apply and verify its actual DOM, then replace with Beta/Apply and verify again. The debug build used 9 rounds, 4 Qev calls and 3 Qev-dispatched actions without recorded tool errors. The release executable repeated the native chain successfully in 17 rounds; it recovered from one model-supplied missing/ambiguous target name and the resulting invalidated observation. Rejection of an incorrect explicit target remains intentional.

## Explorer focused editors and pagination

In a new chat, repeat the Documents/test-folder setup prompt. Confirm it creates exactly one new folder, commits its name, then opens it. If the name already exists, it must leave that folder untouched and choose a genuinely different name. Stop if it cycles through New menus or tries to merge folders.

For a stronger check, use a disposable folder containing at least 180 items. Ask it to search for a deliberately absent name, clear the search, create and name a new folder, then open it. The focused search/rename editor should be present in the current observation even when it lies beyond the first page of controls. A refused stale snapshot must lead to a fresh observation, not blind typing.

For timing comparisons, use the same main model and fresh identical fixtures in Execute/Off/Off/Execute order. Count only verified completions. Measure request-to-response elapsed time (including decision-model loading), main-model rounds, Qev calls, input/cache tokens and tool time. Do not count the smoke harness's fixed idle lifetime as task time, or treat a failed/shortened task as a speedup.
