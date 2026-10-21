# Native control navigation reliability

## Changes

- Native inspection pages through a bounded UIA traversal, rather than slicing the same initial 160 nodes. Each page exports up to 120 elements, with a lookahead and a maximum traversal of 4096 nodes/32 levels. Skipped elements avoid expensive property/text extraction. The cooperative traversal budget remains 1200 ms per observation; a hung individual provider call is bounded by the existing worker deadline. A partial page advances its offset; an exhausted limit with no progress returns null and an explicit limitation. A later page is revalidated against current UI, not a permanent cached tree.
- Only the exported native page has usable element fingerprints. An element from an earlier/unobserved page cannot be acted on using the new snapshot. Automation identity/type/name/bounds/ancestry, password, protected-process and nonce checks remain.
- Tool-result size bounding preserves the provider continuation unless it actually removes elements; then continuation follows the last exported element. It no longer rewinds a provider limit to offset zero.
- Native scroll targets the nearest scrollable ancestor. With no explicit element, it selects the largest visible vertical scrollable descendant. Positive delta moves down, negative up; 120 wheel units is one logical increment. UIA scroll honours magnitude, stops at a boundary, and reports measured before/after percent. Wheel fallback explicitly reports movement as unverified.
- A request-local guard warns after repeated identical clicks/scrolls without observed progress and rejects another native dispatch after three unchanged attempts. Directly measured failed scrolling is not reset by an unrelated playback clock or inspection. Confirmed scrolling or a successful changed strategy resets it. This is not a proof-of-task-completion engine: dynamic UI trees can hide failures, and a legitimate repeated action with no visible feedback can also trigger it. No automatic alternate click or budget increase is performed.
- Screenshot elements include `image_bounds` in screenshot pixels alongside labelled desktop `bounds`. Coordinates are rounded to one decimal. Small native window screenshots retain native resolution rather than being enlarged to 1600 pixels; larger images remain bounded at 1600.
- Control instructions require evidence before claiming encoding, stored line endings, installation status or recency. An installer window alone does not establish prior installation state.

## Validation

`control_navigation_smoke.py` uses disposable Win32 controls: reaches/invokes a button beyond element 160, rejects an unobserved-page target, resolves a list item's scrollable ancestor, selects the default scroll pane, measures magnitude/boundary behaviour, and checks screenshot coordinate conversion. It does not control Spotify or user documents.

Rust regressions cover payload-trim/provider paging boundaries, nonce changes versus real progress, playback-clock changes during failed scrolling, successful scrolling with an unchanged tree, and request/window isolation. A mocked chat test verifies a fourth unchanged native scroll is rejected before another native call.

Existing native/browser, exact-replacement, owned/nested-modal, consent follow-up and real Save As filename-selection fixtures are reused. Foreground keyboard fixtures explicitly report SKIP when Windows denies foreground transfer. A real-model Spotify workflow still requires retesting; fixture success is not a claim that Spotify playback was completed autonomously.

The optimized navigation fixture reached its late control in three inspection pages. One local run measured median inspection 401.6 ms and scroll-plus-observation 1059.8 ms (7 scroll calls). These are fixture timings, not a controlled before/after Spotify benchmark or an end-to-end LLM speed claim. The subsequent compatibility build also preserves the browser schema limit of 600 independently from native offsets up to 4095.

## Follow-up reliability changes

Post-action observations retain the target element's 120-control page. Progress tracking now includes later pages, counts failed native dispatches, and bounds six unsuccessful click/scroll attempts across varying targets in one window. Real observed mutations reset that window counter; dynamic content remains a limitation.

Text insertion uses atomic ValuePattern editing where the document, selection, and independent value agree. Otherwise it retains focused Unicode input. When TextPattern is readable, readback must match the expected edit within 700 ms, allowing CRLF normalization. A mismatch stops without automatically retyping or rewriting unrelated content. This does not guarantee every third-party editor exposes usable readback.

Context checkpointing uses the actual configured context budget instead of a fixed 48KB ceiling. Follow-up tool recall is capped at 16KB and one quarter of configured context, retaining more room for current observations. Hard limits and complete tool exchanges remain enforced. Server cache eviction can still cause misses despite a preserved client prefix.
