# AI step diagnostics

Expand **AI step diagnostics** beneath the current AI conversation. Use **Copy diagnostics** to copy the current response’s bounded JSON report. Each row represents one model request, including tool-selection rounds and the final answer, rather than one physical click. Rows reset on the next question or New Conversation. Numeric reports are also written to VectorDash logs with an AI job identifier.

## Reported values

- `input_tokens`, `cached_tokens`, `output_tokens`: server usage counts. Missing values are `null` (shown as —), not estimated or treated as zero. Cache percentage in the panel is cached/input, only when both counts are valid. A reported zero cache count is a real zero; impossible counts set `usage_inconsistent`.
- `prefill_tokens`, `server_prefill_ms`, `server_decode_ms`: optional server timing counters. llama.cpp `prompt_n` is newly evaluated tokens, not the total input. It is not used to invent a cache count. `tokens_cached` is deliberately ignored because it can describe stored cache rather than tokens reused for this request.
- `first_response_byte_ms`: local elapsed time to the first nonempty response body chunk. `first_token_ms`: elapsed time to the first nonempty streamed text/reasoning/tool delta, ignoring role headers and usage-only events. For nonstreaming JSON responses, first-token time is unavailable. Both include network/queue time and are not pure server prefill measurements.
- `model_ms`: time receiving/parsing the model response, excluding subsequent permission prompts and tool execution. `action_wait_ms` measures that separate tool phase and includes approval waiting. Failed/cancelled model requests still emit a row, with status and HTTP code when known. No raw server error bodies are logged here.
- `text_bytes`, `image_count`, `image_encoded_bytes`, `tool_schema_bytes`, `messages`: request-size measurements, not token estimates. Encoded image bytes do not equal image tokens.
- `matching_prefix_messages`, `previous_messages`, `earlier_context_changed`: exact local message-prefix comparisons within one response’s action loop. These cannot prove a server cache hit/miss; the server’s tokenizer, chat template, model, slot selection and eviction still matter. `tool_config_changed` separately tracks changes to tools/tool choice.
- `screenshots_replaced`, `messages_removed`: VectorDash context edits since the preceding model request. The initial request can also report history removed to fit the context budget.

`status=tools` identifies a model response requesting tools, not proof that a requested action succeeded. Permission denial or a native tool error can still return a valid tool response. Use the normal action observations to establish success.

Only numeric measurements, fixed status labels and flags are copied/logged: no prompts, answers, file paths, URLs, API keys, tool arguments, window text or screenshot data. Temporary message hashes used for comparisons are not exported. At most 130 rows are retained in the UI, covering the existing maximum 128 control rounds plus a final response. Diagnostics make no extra model requests and do not change action limits, tool instructions or context retention.

## Suggested testing

1. Ask the computer tool to open Calculator and calculate `5 * 3`. Expand diagnostics after completion and compare input/cache/output counts with the matching server requests.
2. Run a longer harmless task, such as opening Notepad and editing several lines in a disposable document. Check whether slow rounds correlate with low cached counts, larger text observations, or earlier-context changes. Do not save over existing files.
3. Ask for screenshots during a control task. Later rows should record image counts and replacements when older screenshots are removed; an earlier-context change is a local observation, not a confirmed cache miss.
4. Leave an approval prompt waiting for several seconds. The extra wait should appear under Action wait, not Model time. Also test Stop and a denied action; already completed diagnostics should remain available.
5. Ask a follow-up and start a New Conversation: displayed rows should reset. Each response has a separate job identifier in VectorDash logs.
6. A server that omits usage/cache metrics should show —/null, while timings and request sizes still work. Copy diagnostics to share the affected response. Compare similar tasks rather than treating different requests as a controlled speed benchmark.
