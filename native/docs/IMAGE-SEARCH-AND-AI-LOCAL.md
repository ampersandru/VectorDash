# Image queries and AI Local Search

In Search or Clipboard, paste or drop an image, then press Enter or **Find similar**. Remove image restores the previous text and type selection. Search starts in Images; file-type and source filters still apply. Clipboard searches retained image vectors across the full history, including Saved and Pinned filters.

Image queries use the selected local or remote embedding server and its compatible visual vectors. Existing compatible visual vectors need no index rebuild. Quick indexing and OCR-only indexing do not create image vectors. Image inputs have no text prompt prefix, and image-to-image ranking does not apply keyword bonuses or text rank calibration. Query preprocessing runs on one bounded background worker; results are capped at 100 and collapse multiple visual segments to the best segment per file. Query images and their single cached vector are not persisted as search history.

Search settings can disable image queries. Remote mode sends the selected query image to that embedding server.

In AI chat, **Local Search** appears immediately left of **Web search** and starts disabled. A tool-capable LLM can request a concise local query and receive at most five indexed paths and snippets, with page/time metadata where available. Sources and exclusions use the normal search policy. Quick names work without embeddings; Full indexed content can also use semantic search. Results are untrusted source data, not instructions. The tool has no file editing, execution, or full-file reading operation. Returned snippets are sent to the configured AI server.

Local and Web tools share the existing bounded two-round tool loop. Every requested call receives a paired tool response, including errors and skipped excess calls. Local retrieval runs outside the async HTTP runtime and uses the normal query cancellation generation.

## Verification

- Native regressions cover visual-only ranking, video/file deduplication, type and exclusion filters, Quick-source exclusion, FP16/FP32/streaming retrieval, cancellation and dimension mismatches.
- Clipboard fixtures include 605 images and verify the oldest exact match is found, plus Saved/Pinned/Text filtering and protected embedding payloads.
- HTTP tests verify bare image inputs, normalized/cached query vectors, Local tool schemas, paired errors and a complete Local tool answer cycle.
- `native/scripts/image_local_search_smoke.py` uses isolated profiles and a local mock server, with clipboard capture disabled. It checks attachment without automatic inference, Search results, old Clipboard results, disabled image queries, and combined Local/Web tool access.
- A live configured EmbeddingGemma 2 server returned 768-dimensional vectors for three generated test pictures and a crop. The crop ranked its original first (0.989 similarity; other pictures 0.803 and 0.811). The first request took 2.7 seconds and subsequent requests 40–60 ms. This small synthetic check validates the image pipeline, not broad real-photo relevance or general performance.

Both features are BETA. Real-photo relevance, OS clipboard formats, RAW query images, and mixed-monitor wake/drag behavior still benefit from user testing.
