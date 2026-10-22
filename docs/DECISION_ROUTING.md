# Decision routing and latency

[OpenAI Decisions guidance](https://developers.openai.com/api/docs/guides/decisions) distinguishes typed classification/selection from generated explanations and tool arguments. VectorDash applies those principles to Qev through its supported `/v1/systemone` interface; it does not send local Qev requests to OpenAI's `/v1/decisions` endpoint.

The [Qev model card](https://huggingface.co/labqoat/qev-0.8b-GGUF) confirms it returns probability distributions rather than generated text. Main-model plans must still provide text, keys and explicit postconditions. A fixed choice set includes escalation; confidence and runner-up margin remain enforced. No decision bypasses consent, snapshot identity, ownership or protected-target checks.

The latency correction uses text evidence for a uniquely named accessibility/DOM target. Qev still scores that choice, but the runner skips a redundant screenshot capture, resize and vision prefill. Ambiguous candidate sets still receive the optional vision image. Diagnostics expose Qev call counts, inference time and actual vision-image count: enabled vision with zero images can be an intentional text-grounded decision.

Independent questions about one observation can share input in a decision request. Dependent input steps cannot be pre-scored against an old screen: they must observe and verify each result. Existing bounded plans allow up to eight explicit steps in one main-model call and stop on failed verification. Reusing successful facts, caching unchanged escalations, compact candidate sets and immutable prompt prefixes reduce additional main-model work. Dynamic screen changes always invalidate stale targets.

Optimization must be evaluated on successful full workflows. Request-to-response timing includes model loading, capture, Qev and main-model calls. Report failed/incomplete runs separately. These are small classification models, not a drop-in autonomous desktop planner; target scoring alone can add latency when the main model still plans every action. Benchmark both modes on the same workload and server instead of assuming a speedup.

A local CUDA calibration experiment compared four question wordings and richer input-state descriptions on correct, missing and duplicate controls. More verbose context degraded acceptance of otherwise valid targets at the unchanged 0.9 threshold, so those prompt changes were reverted rather than shipped or compensated for by lowering safety thresholds. The retained change removes redundant visual work without changing the trained selector framing.
