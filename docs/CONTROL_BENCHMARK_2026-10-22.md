# Local control comparison — 22 October 2026

The reported Explorer failure consumed 129 main-model rounds: approximately 526 seconds in the main model and 88 seconds in tool work, including 27 Qev calls (31 seconds) and eight Qev dispatches. It did not finish. These timings overlap: Qev time is part of tool time.

## Corrections

- Native observations include one owned, non-password focused editor independently of the current traversal page. Explorer search and inline rename fields can otherwise be beyond hundreds of file rows. The added target has a snapshot-local identity; input still revalidates ancestry, identity, type and geometry. This does not expand traversal limits or authorize blind typing.
- Instructions tell the model to preserve successful creation progress, distinguish an item created during the request from an existing-name conflict, and prefer the native new-folder shortcut over looping through transient New menus.
- A short future-action-only final response is now rejected as unfinished, even after earlier inputs were dispatched. This is a completion sanity check, not a semantic task verifier.

- Browser click/type/key dispatch acknowledgements are now exposed consistently, rather than only being copied to the top level for scroll actions. This fixes false incomplete responses, inaccurate observed/dispatched diagnostics, and loss of input progress when Qev is Off. An initial browser comparison exposed this defect and is excluded from the final timing table.

## Method

Used the configured local `LLM:instruct` API with real requests and the configured managed local Qev model with vision. Credentials remained in memory. Fresh temporary application profiles, identical prompts apart from unique fixture identifiers, 24 control actions maximum, 2,048 response-token cap, and the same configured context limit. Only disposable fixture controls and a localhost browser page were exposed to the model; other applications were excluded/rejected by the test proxy.

The native workflow sets and verifies Alpha and Beta, turns a test switch off, closes an owned dialog using the second of two identically named Save buttons, then sets and verifies a final value. Both intermediate and final states must match. The browser workflow sets Message to Alpha and Beta, clicking Apply and verifying both page states. No restaurant, Spotify or user Documents data was changed.

Elapsed time is the debug transcript request-to-response interval, including Qev request startup and tool work; it excludes the harness's fixed idle period after completion. Model, action and Qev time are not additive independent categories. Both comparisons used the same debug binary for both modes, with the actual configured Dedicated GPU / CUDA0 selection copied into disposable profiles. Earlier CPU-only profile timings were a benchmark setup error and are excluded. The final optimized release is checked separately. Do not compare native and browser times directly as equivalent tasks. Cached-token reuse is reported, not assumed to be identical between runs.

## Results

| Test | Mode | Elapsed | Rounds | Qev calls | Input tokens | Cached tokens |
|---|---|---:|---:|---:|---:|---:|
| qev-gpu-native-execute-1 | Execute | 40.30 s | 15 | 9 | 117,869 | 105,655 |
| qev-gpu-native-off-1 | Off | 27.35 s | 12 | 0 | 68,599 | 59,854 |
| qev-gpu-native-off-2 | Off | 29.73 s | 12 | 0 | 69,473 | 60,706 |
| qev-gpu-native-execute-2 | Execute | 43.43 s | 19 | 9 | 168,550 | 155,056 |
| qev-gpu-browser-execute-retry | Execute | 26.49 s | 15 | 6 | 60,867 | 55,343 |
| qev-gpu-browser-off | Off | 15.08 s | 10 | 0 | 23,683 | 20,881 |

Native: Execute averaged 41.86 seconds versus 28.54 seconds Off (+13.32 seconds; +46.7%). All listed runs completed verified intermediate and final states.

Browser: Execute averaged 26.49 seconds versus 15.08 seconds Off (+11.41 seconds; +75.6%). All listed runs completed verified intermediate and final states.

One earlier browser attempt was rejected by the disposable test proxy allowlist (HTTP 502) and is excluded from timing; it is not counted as a successful task. This is a small controlled sample, not a general speed guarantee. These rows use the corrected Dedicated GPU / CUDA0 settings; earlier accidental CPU-only test profiles are excluded. Qev currently ranks and grounds targets inside a main-model-driven loop. It does not replace the main model for every action; inference and refresh overhead can exceed any saved planning rounds. Faster autonomous execution needs bounded local action plans that replace main-model turns while retaining verification and permission boundaries.

Validation: 411 unit tests passed (24 ignored), focused-editor pagination regression passed in the debug helper; the final optimized helper probe was skipped when Windows refused foreground acquisition, and real local-model native/browser fixture runs verified their required states. A separate real Explorer automation probe was blocked by Windows foreground policy before input, so the full real Documents workflow is not claimed as a passing automated retest. Re-test that workflow interactively using the supplied prompts.
