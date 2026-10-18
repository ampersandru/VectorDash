# 0.3.41-beta.5 local test build

Includes the post-beta.4 search, icon and mapped-folder fixes from commit 148d48b, plus:

- Fix: AI batches of more than two Web searches no longer fail during parsing; at most two searches execute per round and every additional call gets a matching tool response explaining the limit.
- Feature: Web settings offer a separate AI web search provider selector; it can follow the saved Web default or explicitly use any configured provider independently of the Web tab picker.
- Hardening: Requests ask compatible models to disable parallel tool calls; streamed batches, sparse call indexes, tool arguments and total responses remain bounded.

Tests cover streamed and non-streamed batches of five calls with sparse indexes for both SearXNG and Degoog. Each case verifies only two provider requests occur, all five tool responses have matching IDs, the independently selected AI provider receives the searches, and a final AI answer is returned. Settings tests cover save/load, legacy defaults and fallback if a provider is removed.

Manual: configure two providers under Web, select one for ordinary Web and the other for AI, save, then enable Web search in AI and ask a question requiring current information. Repeat with the AI provider reversed. Follow-up answers and Stop should continue to work.

This setup is provided locally for testing; building it does not publish a GitHub beta release.
