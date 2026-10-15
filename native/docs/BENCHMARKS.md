# Native preview measurements

Measured on the development Windows 11 PC, with an NVMe-backed copied index containing 1,076,786 files and 14,123 vectors. Semantic queries used an already running local GPU llama-server. Four searches per process: first includes vector-cache preparation, warm is the median of the next three. These are backend timings; UI debounce, cold model loading, network latency and rendering are additional. OS disk caches were warm. Values are observations, not performance guarantees.

| Storage | Profile | Search | First ms | Warm ms | Peak backend MiB |
|---|---|---|---:|---:|---:|
| encrypted | Low memory | Names | 7.83 | 9.21 | 16.8 |
| encrypted | Low memory | Semantic | 165.58 | 150.62 | 20.3 |
| encrypted | Low memory | Both | 177.58 | 178.77 | 20.5 |
| encrypted | Balanced | Names | 6.73 | 9.32 | 43.2 |
| encrypted | Balanced | Semantic | 348.17 | 31.66 | 88.7 |
| encrypted | Balanced | Both | 321.01 | 46.85 | 88.9 |
| encrypted | Fast search | Names | 6.29 | 9.02 | 78.7 |
| encrypted | Fast search | Semantic | 184.92 | 31.29 | 124.2 |
| encrypted | Fast search | Both | 200.27 | 45.39 | 124.2 |
| plain | Low memory | Names | 4.75 | 7.18 | 15.4 |
| plain | Low memory | Semantic | 72.63 | 67.11 | 18.8 |
| plain | Low memory | Both | 92.72 | 81.42 | 19.0 |
| plain | Balanced | Names | 4.6 | 7.26 | 42.1 |
| plain | Balanced | Semantic | 119.95 | 21.77 | 87.3 |
| plain | Balanced | Both | 133.66 | 33.19 | 87.4 |
| plain | Fast search | Names | 4.14 | 6.72 | 77.3 |
| plain | Fast search | Semantic | 109.22 | 21.48 | 122.8 |
| plain | Fast search | Both | 118.5 | 33.27 | 123.1 |

Names query: `forza`; semantic: `images of dogs`; Both: `forza`. All searches returned results. RAM includes the backend process, excludes GUI, parser helpers and model server. Low memory streams vectors; Balanced and Fast search retain a bounded compact vector cache. The filename catalog and readable document text stay on disk.

## Indexing and launch

- Generated approximately 2 MiB text file: 43.2 seconds, 1,309 passage vectors, 328 batched embedding requests. First request at 0.52 seconds; median request gap 0.87 ms, maximum 10.1 ms. Peak backend 32.0 MiB. The forwarding measurement proxy adds connection overhead. Model inference dominates this test; it does not establish the cause of the third-party report.
- 20,000 generated small files, encrypted Quick indexing: 0.63 seconds counting off, 0.67 seconds counting on. Unchanged rescans: 0.32 / 0.37 seconds, with all 20,000 reused and zero changed. Backend peak about 29 MiB. This local synthetic tree does not represent slow disks or disconnected network sources.
- Empty native GUI: 115.4 MiB peak, no model loaded; launched, drew 89 frames and exited after a three-second smoke interval. Total process lifetime 3.52 seconds, including that deliberate interval. This is not a time-to-first-paint measurement.
- Copy import of the existing encrypted million-file index: 18.4 seconds. Original database was opened read-only.

## Validation and limits

18 native regression tests passed. The setup was installed into an isolated folder, its GUI launched, and the installation uninstalled successfully. Portable launch and absence of packaged Python/user data were checked. A test model terminated after both normal and forced parent-process exits. Packaged AVIF and JPEG XL decoding fixtures passed. Audio streaming emits separate 30-second pieces; video frames retain their sampled times. A silent-video fixture caught an audio-absence failure, which was fixed by inspecting audio streams before extraction.

No matched Everything/Listary benchmark has been run; no parity claim is made. Compare identical folders and queries, cold/warm launch and search, result recall, memory including helpers, and initial/incremental indexing. Everything journal indexing and content embeddings are different workloads.

This is a migration preview: standard-dialog path insertion is implemented but every custom/elevated dialog has not been tested. Explorer activation opens quick search; it is not a shell-extension embedded results view. Specialized XLSX cell reconstruction, legacy retrieval-evaluation parity and animated tray visuals need further work. Image/PDF GUI previews use native decoders in process; indexing parsers have separate low-integrity/job-limited workers. Retain the stable release until the native preview is validated for your workflows.

Preview 0.3.1 restores appearance/settings and background CPU/GPU scheduling. See [update details](0.3.1.md). Earlier timing numbers above describe the original native benchmark build; the retrieval algorithm and bounded cache profiles are retained.

## 0.3.24 stabilization comparison

Measured October 15, 2026 on the development Windows 11 PC against 0.3.23. A disposable NVMe-backed catalog contains 10,000 names and 10,000 768-dimensional vectors. Each process performs 12 searches; warm values are the median of the final 11. OS disk caches were not flushed. A local deterministic embedding service adds 5 ms per request, so these numbers compare client/database overhead, not inference quality or cold model loading. Names means File/Folder Names; Both means Hybrid.

| Storage | Profile | Search | 0.3.23 warm ms | 0.3.24 warm ms |
|---|---|---|---:|---:|
| Plain | Low memory | Names | 0.85 | 0.86 |
| Plain | Low memory | Semantic | 48.07 | 45.13 |
| Plain | Low memory | Both | 48.50 | 46.05 |
| Plain | Balanced | Names | 0.84 | 0.91 |
| Plain | Balanced | Semantic | 12.67 | 10.32 |
| Plain | Balanced | Both | 14.01 | 11.11 |
| Plain | Fast search | Names | 0.89 | 0.95 |
| Plain | Fast search | Semantic | 13.21 | 10.38 |
| Plain | Fast search | Both | 13.57 | 11.37 |
| Encrypted | Low memory | Names | 1.14 | 1.15 |
| Encrypted | Low memory | Semantic | 102.97 | 101.53 |
| Encrypted | Low memory | Both | 105.28 | 100.53 |
| Encrypted | Balanced | Names | 1.27 | 1.11 |
| Encrypted | Balanced | Semantic | 13.65 | 11.16 |
| Encrypted | Balanced | Both | 14.81 | 14.42 |
| Encrypted | Fast search | Names | 1.16 | 1.16 |
| Encrypted | Fast search | Semantic | 13.23 | 10.78 |
| Encrypted | Fast search | Both | 14.47 | 11.67 |

Names remained approximately 0.8–1.3 ms. Source and metadata filtering happens before candidate limits. Balanced/Fast retain a bounded vector cache; Low memory streams vectors from disk, and encrypted streaming has a measurable cost.

| Indexing fixture | 0.3.23 | 0.3.24 | Peak process-tree memory before / after |
|---|---:|---:|---:|
| 128 small text files, batch 1 | 3.246 s | 0.942 s | 31.8 / 19.6 MiB |
| 128 small text files, batch 4 | 3.198 s | 0.315 s | 31.6 / 19.5 MiB |
| 72-page image PDF extraction | 2.507 s | 2.389 s | 64.8 / 47.2 MiB |

Indexing rows use the median of three fresh runs. Batch 4 reduced embedding requests from 128 to 32. PDF extraction used the bundled PDFium, visual pages enabled and OCR disabled; no embedding server was involved. Memory excludes Python's fixture driver and server memory/VRAM.

The configured remote EmbeddingGemma 2 backend indexed 24 generated text files and three geometric images in 4.764 s before versus 1.195 s after (one fresh run each, including service latency). All 18 Semantic/Hybrid topic queries returned the expected topic first in both builds. Corresponding 768-dimensional vectors had cosine similarity above 0.999999. This small generated corpus detects regressions, but does not establish broad retrieval quality. Pinned generated text/image vectors also test single-versus-batched output ordering against that backend; other quantizations and backends require separate qualification.

Reproduce with `native/scripts/stability_benchmark.py`; it creates disposable data homes and optionally reads only embedding configuration from `--live-settings`. It never opens the user's index. Raw local timing reports include cold and p95 timings. Network variability affects individual real-server query timings.

`--convert-encryption on|off` creates a converted database and retains the original, matching the settings conversion behavior. Back up before using it on an important index. Resume checkpoints reuse embedding work but currently re-extract earlier pages/chunks. Interactive searches have priority between batches; an already-running server batch is not preempted. Without a usable NTFS journal, startup catch-up may still scan sources to recover offline changes.

Validation for this build: 147 standard Rust tests passed, plus explicit PDF streaming/cancellation, native H.264 decoding, and real-server reference checks. End-to-end process interruption retained eight of 137 chunks and reduced restart requests from 35 to 33, without publishing partial content. HTTP 503 recovery completed on the next update; one rejected input did not prevent four valid files from indexing. A configured Git fsmonitor hook was not executed. Main/popup Search and AI and the Sources page passed isolated launch/render checks. Three package-validation tests passed. These checks do not replace broad file-format fuzzing or testing every hardware/backend combination.


## 0.3.25 Beta 2 validation

Compared with PR #16's preceding preview-fill build (6174826) on the same machine. These are disposable generated indexes, never the user's index. Search timings use one cold query followed by seven warm repeats; OS caches were not flushed. All three memory profiles, Names/Semantic/Hybrid modes, and plain/encrypted storage were exercised.

### Cache precision: 20,000 vectors, 768 dimensions

| Balanced semantic cache | FP32 median / p95 | FP16 median / p95 | Peak process RSS FP32 / FP16 |
|---|---:|---:|---:|
| Plain index | 20.270 / 23.799 ms | 22.313 / 24.470 ms | 120.53 / 91.29 MiB |
| Encrypted index | 22.411 / 26.639 ms | 23.299 / 28.552 ms | 121.36 / 92.50 MiB |

Four seeded queries per semantic mode used exact prototypes, 800 close neighbors, and unrelated normalized vectors. Top-1 agreement and top-10 set overlap were 100% across both cached profiles and encryption modes; the maximum shared score difference was 0.00000221. Low memory mode ignores cache precision and produced identical scores. This is a cache/pipeline test with fixed mock query vectors, not a model-inference benchmark. FP16 normalization is cached per row; conversion/scoring stays FP32. Vector payload memory halves; total process memory does not.

On the configured real remote backend, 24 generated text files and three geometric images retained the expected top topic for all 18 Semantic/Hybrid queries with FP32 and FP16, and calibration off/on. The index was copied for each check; no embeddings were rewritten. This small corpus cannot establish general relevance improvements or guarantee unchanged close rankings on other data.

### Indexing and resilience

128 tiny text files with a fixed 5 ms mock service delay, median of three fresh runs: batch 1 1.0355 → 1.0129 s; batch 4 0.3264 → 0.3254 s; batch 16 0.1384 → 0.1488 s. For tiny files there is little preparation to overlap, and added scheduling can cost time. The 72-page PDF extraction control was 2.3995 → 2.4507 s, with 72 pages in both builds. Neither table is evidence of faster model inference.

A static two-scene, six-second video retained both scenes and the three-second transition with deduplication enabled. FFmpeg yielded six candidates and two retained frames; the native decoder yielded four candidates and two retained frames on this generated long-GOP fixture. Deduplication does not reduce candidate decoding. Native seek behavior is codec dependent; use the existing FFmpeg option when it misses desired moments. Flat scenes with different colors are not treated as duplicates.

Long-file tests verify that a small file embeds before the long file finishes, with unique final vectors and no pending jobs. Yielded chunks stay in durable staging; old complete content remains searchable until an atomic final replacement. PDF page resume, cancellation, transient 503 recovery, rejected-file isolation and Git-hook non-execution passed. Forced termination after eight staged chunks retained them, completed 137 vectors after restart, and used 33 requests versus 35 uninterrupted; no incomplete file content was published.

161 standard Rust tests passed, plus explicit PDF and native H.264 checks. Main/popup image/video grids rendered, selected row 20 scrolled into view, and simulated Windows 10/no-glass paths launched. These checks run on Windows 11; a real Windows 10 machine remains required before removing the beta label.

Reproduce cache measurements with `native/scripts/cache_precision_benchmark.py`; the before/after controls use `native/scripts/stability_benchmark.py` and interruption checks use `native/scripts/stability_faults.py`. RSS excludes model-server RAM and VRAM. Learned estimates cover known jobs only, require three samples per content kind, and can be affected by server load and idle waits.

The one-batch-ahead image fixture uses 24 generated 1600×1000 PNGs, batch 4 and an 80 ms mock embedding service. After removing per-image subprocess startup for bounded Rust-decoded images, median indexing over three runs was 1.4472 → 0.5908 s, with six requests and all 24 image vectors present in every run. This measures overlap and pipeline overhead, not GPU inference; OCR and other codecs retain isolated workers.

Fair scheduling slices PDFs at 32 new records with page resume. Other documents begin at 32 records and grow their turns as staged progress increases, limiting repeated extraction. Audio/video keep one streaming decode pass; they still yield model access between embedding batches, but do not interleave separate indexing files mid-media. Full timestamp-based media resume is not part of this build.

The compatibility audit guards Windows 11 DWM attributes and uses a rounded tinted fallback on Windows 10. See Microsoft's [DWM corner requirements](https://learn.microsoft.com/windows/win32/api/dwmapi/ne-dwmapi-dwm_window_corner_preference) and [DWM attributes](https://learn.microsoft.com/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute). Media Foundation remains delay-loaded with the existing FFmpeg alternative. Imported APIs and forced 19045 branches were checked on the development machine; this is not a Windows 10 device certification.

Footer validation used a deliberately slow embedding endpoint: the blue outline and waiting notice remained active until a response, then returned to the result count and measured search time. Main/popup checks covered three-second notice expiry, measured status centering, AI request elapsed time and real test-audio read-aloud controls. The compact popup footer now reserves a 40-point row for its 35-point Open full app button, with padding inside the rounded window.

The final candidate also recorded 0.0% process CPU in three consecutive four-second tray-idle samples after warm-up, with no sources, clipboard capture or background indexing enabled. This short idle observation does not cover every enabled background service. Final package checks verified installer/portable manifests and four release-gate tests passed.


## 0.3.25 Beta 3 follow-up

161 standard tests passed. A fully buffered eight-second silent WAV finished before the 13-second observation without pausing; pausing from seconds four to eight kept playback active at second 13, after resume. This checks the actual player rather than only the UI flag.

Main/popup checks covered independent grid/list widths and persistence, real H.264 frame thumbnails, and constrained Filter panels. With saved divider ratios of 0.75 for lists and 0.40 for grids, preview widths were 222 and 532.8 points respectively, with both saved values retained. The compact Filter case stayed within a 940×500 viewport (panel bounds 598,101 to 932,492) and returned to height 194 on close. File type options now open from the input bar, with enough menu height for all eight choices.

Video thumbnails use one frame on demand in the existing two-worker, eight-slot queue. The RAM-only texture cache holds at most 96 thumbnails with maximum side 128 pixels, or at most 6 MiB RGBA pixel payload; codec and GPU allocation overhead are additional. No persistent thumbnail cache is created. Unsupported decoding falls back to the file icon.

## Beta 4: inaccessible network subfolder regression

The same read-only NAS subtree was Quick-indexed into isolated profiles using the packaged Beta 3 and Beta 4 executables, with counting enabled. A child directory returned Windows SMB error 59 while the root and sibling directories remained readable. Beta 3 stopped at that child: 0 files counted/indexed and no match for a known sibling file. Beta 4 counted/indexed 1,255 files, skipped four inaccessible paths, and returned the expected sibling search result in a 0.74-second scan. No changes were made to the share or the active application index. Timing is specific to this small, warm-cache subtree and does not establish general NAS throughput. Unit coverage additionally verifies that a genuinely unavailable source retains its existing names and that the network error codes only circuit-break an unreadable root.
