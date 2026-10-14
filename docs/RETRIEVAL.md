# Retrieval checks and coverage

VectorDash combines exact words and SQLite FTS5/BM25 with semantic similarity. Exact identifiers get extra weight. Each passage, PDF page, image, video frame and audio window is indexed separately; search returns the strongest matching location instead of compressing a whole long file into one vector.

EmbeddingGemma 2 text, image and audio vectors use the same model space. OCR text gets an additional text vector. Search compares only matching dimensions and the configured model signature; dimension mismatches require rebuilding. It adjusts each modality by its per-query mean across files so a long movie does not dominate that average merely by having more segments. A score is a ranking signal, not a probability. There is no LLM-as-judge stage and no claim that vectors from unrelated models become comparable through normalization.

Explicit media requests such as `images of dogs`, `where they talk about sleep` and `zebra in a video` narrow results to the requested media. The category dropdown and excluded extensions let you narrow other searches. Exact matches continue to count in filenames, document text and OCR.

## Checks performed for 0.2.0

- 82 automated tests covering retrieval, Office extraction, migration, downloads, security, scheduling and Windows integrations.
- 1,545 generated indexed files: 1,500 notes, 14 screenshots, 30 PDFs and one project with 68 functions. All 111 exact/OCR queries and 68 code queries returned their expected file first. Warm keyword search median was about 29 ms on the development computer, excluding typing debounce and model inference.
- The longest generated PDF contained 463 pages; its unique clause was found on page 463.
- One hour of generated tone produced 144 overlapping audio segments, reaching the end at 3,600 seconds. A one-minute generated video produced 20 sampled frames, with the final frame at 57 seconds. These checks validate coverage and timestamps, not speech or natural-video recognition.
- Six live EmbeddingGemma 2 BF16 checks passed on a small fixture set: English found Bengali and Arabic notes, Bengali and Arabic found an English note, an invoice matched exactly, a generated speech podcast returned the window containing the sleep discussion, and a known dog photo ranked first for `images of dogs` over an unrelated screenshot. This is a regression check, not a broad accuracy benchmark.
- Native UI checks cover score/reason display, Space preview return, shared filters and MCP start/stop. Packaged checks cover protected indexing, previews, the included decoder and authenticated MCP search.

Run `windows/qa_retrieval.py` to reproduce the generated-file checks. `windows/qa_live_model.py` needs a running multimodal llama-server at localhost:8787 and uses an optional development photo only if present. Private fixtures and indexes remain under ignored `windows/.smoke`; they are not published.

## File coverage

| Type | Handling |
| --- | --- |
| Text, Markdown, CSV, logs | Overlapping passages of about 1,800 characters |
| PDF | Page and passage locations; OCR for scanned pages; matching word highlights |
| DOCX | Body, header and footer text; passage preview |
| XLSX | Worksheet names, cell addresses, shared strings and stored cell values; no formula execution |
| PPTX | Slide text and slide number; passage preview |
| Photos/screenshots | Image embeddings plus optional Tesseract OCR for exact text |
| Camera RAW (experimental, off by default) | ARW, CR2/CR3, DNG, RAF and other LibRaw-supported formats; embedded JPEG/RGB preview first, optional reduced-resolution development; Full sources only |
| Audio/music | Overlapping 30-second non-silent windows; semantic retrieval and time locations; no transcript or exact lyric guarantee |
| Video/movies | Frames every few seconds and on scene changes, optional frame OCR, plus audio windows; long videos can take substantial time to index |
| Code | Opt-in project scope; functions/passages with line locations; respects Git ignores |
| ZIP | Safe entry-name manifest only; no recursive extraction or execution |
| Safetensors | Bounded JSON metadata only; tensor weights are not embedded |
| RAR, legacy DOC/XLS/PPT, other weights/binaries | Filename search only |

RAW support uses a pinned LibRaw 0.22.2 helper and the existing image embedding, thumbnail and Quick Look paths. Search results retain the original RAW path. Enable it in Sources, save, and run Update index: RAW-specific recipe stamps upgrade unchanged filename-only entries without rebuilding unrelated content. Quick sources and disabled RAW support never decode camera files. Unsupported/corrupt RAWs remain searchable by name and log a warning; unchanged failures are not repeatedly retried unless Retry failed extraction is enabled.

RAW has a separate input-size limit (default 256 MiB, configurable 16–1024 MiB), one decoder at a time, a 512 MiB process budget, 128 MiB LibRaw raw-buffer cap, 16MP decoded-image limit and 45-second limit per attempt. Output is reduced to a 2048px longest edge. Development is optional and restricted to sensor data up to 50MP; half-resolution output does not remove the need to unpack the sensor. Camera variants, unsupported preview encodings and files exceeding these limits can remain filename-only. Originals are read-only. This first preview writes no persistent derived-image cache; adding a bounded cache that follows index encryption is a planned improvement. Multimodal remote embeddings send the derived image to the configured server just like ordinary photos.

Validation uses CC0 camera samples from [raw.pixls.us](https://raw.pixls.us/): Sony ILCE-7S and ILCE-6700 ARW (the latter over 32 MiB), Canon EOS 40D CR2, Fujifilm FinePix S5000 RAF and a Blackmagic Micro Cinema Camera DNG with no usable embedded preview. Real decoding, Unicode paths, uppercase extensions, a rotated QA copy, failure retention and unchanged rescans are checked. Embedding transport/search plumbing uses a deterministic 768-dimensional test server; this is not a RAW visual-relevance benchmark. Fixture originals and test indexes are not bundled with releases.

Abstract recollections, natural photos and long-media accuracy still depend on the model, input quality and sampling. The generated test counts above must not be presented as a real-world accuracy benchmark or as DigUp's published results. Update the index after upgrading to enable the new extraction and security options.
