# FileDig indexing and search comparison

Reviewed FileDig commit `d88af733656b7429f87cfc1eb6295e42d48aeb2e` from https://github.com/rupeshs/filedig. This was a source review, not a head-to-head benchmark on the same machine and files.

FileDig batches up to eight text inputs per embedding request, uses 1,200-character overlapping passages and downsizes images to 896px. Its configured model/projector are Q8_0. Its supported content is text, DOCX, readable PDF pages and images. Images receive a visual vector and filename keywords; this extraction path does not run OCR, audio transcription/embeddings or video-frame sampling. Its SQLite transactions commit per file, and its Rust float32 vector cache keeps vectors ready for search. Search scores a top-200 semantic candidate set and merges a bounded FTS5 keyword result set before resolving results.

Primary source: [indexer.rs](https://github.com/rupeshs/filedig/blob/d88af733656b7429f87cfc1eb6295e42d48aeb2e/src-tauri/src/indexer.rs), [engine.rs](https://github.com/rupeshs/filedig/blob/d88af733656b7429f87cfc1eb6295e42d48aeb2e/src-tauri/src/engine.rs), [search implementation](https://github.com/rupeshs/filedig/blob/d88af733656b7429f87cfc1eb6295e42d48aeb2e/src-tauri/src/lib.rs), [vector cache and database](https://github.com/rupeshs/filedig/blob/d88af733656b7429f87cfc1eb6295e42d48aeb2e/src-tauri/src/db.rs).

VectorDash does more work when OCR, PDF page visuals, audio and video are enabled. Its selectable BF16 model is also larger than Q8_0. FileDig's README performance figures use its own machine and workload and should not be treated as comparable measurements.

## Changes made in VectorDash 0.2.8

- Reuse a read-only in-memory SQLite connection until the protected snapshot changes, instead of deserializing the full index for every query. Windows account encryption at rest remains enabled.
- Cache parsed segment locations and calculate repeated path exclusions once per file/kind per search.
- Do not embed unsupported filenames or launch a parser just to create their filename-only record. Exact filename lookup remains available. Older filename-only vectors are excluded from semantic scoring without forcing a rebuild.
- Retain query-vector caching and the normalized NumPy matrix cache already present in VectorDash.

A synthetic 25,000-segment benchmark of warm SQLite read access measured 17.49ms per read using the previous copy-per-read path versus 0.01ms with the reused read connection. This isolates database access; it excludes embedding, result ranking and UI time and does not establish an equivalent improvement in total search latency.

Further candidates are small text embedding batches with per-input fallback, a bounded semantic candidate set that preserves exact matches, and reusable isolated parser workers. These require additional latency, accuracy and containment testing. They are not implemented in this update. No FileDig implementation code was copied.

## Follow-up changes in 0.2.9

General queries previously constructed candidates from almost every passage with cosine similarity over 0.12. Ranking alone on the user’s 34,672-segment index measured 4,379–4,435ms for two queries, using an injected fixed query vector to exclude model and server time. Detailed ranking now operates on the top 400 semantic candidates per channel plus all FTS keyword hits. This preserves keyword recall and prevents text candidates from crowding out picture candidates.

Visual queries exclude separate image OCR text vectors when the image has a media vector; the media record still retains OCR text for exact matching. File/folder keyword contributions can be enabled explicitly; they default off. Result rows are 66px tall, thumbnails decode on two background workers, and generic icons are local SVGs rather than synchronous Windows Shell lookups. Native UI testing displayed 100 cached-thumbnail rows in 67.4ms. These are isolated measurements, not an end-to-end comparison with FileDig.

After the candidate change, the same injected-vector tests measured 174.0ms and 178.3ms for the general queries, and 39.8ms for the image query. Model inference was mocked with the same existing vector in both tests; these measurements establish ranking cost only, not result accuracy or complete user-visible latency. The live model endpoint became unavailable before the complete pipeline could be retested. Debug stage timings remain available for live diagnosis.
