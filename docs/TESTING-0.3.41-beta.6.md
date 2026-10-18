# Local beta.6: AI Web round exhaustion and concurrency

The beta.5 batch parser fix did not address the separate two-round tool budget. The previous final request still exposed the tool schema with tool_choice=none; a model ignoring that preference triggered an abort. The final request now removes the schema, tool-choice and parallel-call flags, converts tool replies into clearly marked untrusted source text, and requests an answer to the latest question. If the server still emits tool calls or no final answer, the app displays an explicit notice with available sources instead of discarding the inquiry. It never invents a summary or grants additional searches.

Web settings add Concurrent AI web searches: 1 by default (serial), or 2. Both settings retain at most two searches per round for two rounds. Actual overlapping HTTP requests are controlled, not just the model parallel-call hint. Invalid saved values are clamped to 1–2 and legacy settings default to 1.

Tests exercise both permitted tool rounds and assert the final request contains no tool schema, tool roles or tool-call history, while retaining the original question and evidence. An intentionally noncompliant model must return sources without more requests or failure. A separate live local HTTP test proves serial requests with 1, overlapping requests with 2, and stable tool reply ordering even when completion order differs. Existing multi-provider streamed/JSON five-call batch tests remain required.

Manual: choose 1 under Web, save, and repeat the exact query that failed. Test 2 against a provider that permits simultaneous requests. Check Stop during searches, a follow-up question, and a query requiring current information. The independent AI provider setting remains available. This build also includes all previous beta.5 fixes and is a local installer, not a published GitHub prerelease.

Verification: 283 native tests passed, 18 environment/opt-in tests skipped. Final-response coverage includes ordinary text, unwanted tool calls despite tool removal, and an empty server reply.
