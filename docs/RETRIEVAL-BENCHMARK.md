# Retrieval evidence regression benchmark

Issue [#23](https://github.com/ampersandru/VectorDash/issues/23) exposed a distinction between finding a file and finding the evidence requested inside it. These tools keep those scores separate. They do not claim that synthetic performance predicts real-world quality.

## Larger deterministic corpus

Use Python with Pillow installed. Write to a **new** directory outside your ordinary indexed folders:

```powershell
python native/scripts/generate_retrieval_corpus.py windows/.smoke/retrieval-corpus --ffmpeg .build-tools/ffmpeg-rust/ffmpeg.exe
```

This creates 156 files and 288 queries: 120 near-duplicate maintenance documents (24 Unicode RTFs), 24 identifier/shape images, and 12 silent forward/reverse video controls. Without `--ffmpeg`, it creates 144 files and 264 queries. Never add real personal documents to a published result bundle. The generator refuses an existing output directory.

Full-index `corpus` in an isolated test profile, enable English OCR, use a video sampling interval of at most five seconds, and record the actual settings, model revision/quantization, hardware, and app commit. Run each query through CLI or MCP and store JSONL records shaped as `{ "query": { "id": "Q001" }, "response": { "results": [...] } }`. Keep native result ordering. Report cold/warm latency separately using the same interface and cache conditions for comparisons.

```powershell
python native/scripts/retrieval_benchmark.py --truth windows/.smoke/retrieval-corpus/ground-truth.json --responses results.jsonl --output scores.json
python -m unittest discover -s native/scripts -p test_retrieval_benchmark.py
```

Default scoring keeps cross-category distractors, as universal search would. `--category-scoped` reproduces issue #23's category-filtered protocol; label that choice in reports. Missing/duplicate query responses are errors, not silently dropped samples.

## Scores and evidence

- **Top1/Top5:** first acceptable file after file deduplication.
- **Evidence Top1:** first-ranked file is correct and supporting evidence has the required modality, timestamp range, and (for exact identifiers) actual matching text. Annotated queries are counted separately.
- **Components:** timestamp, modality, exact-text, page and order checks also have separate counts. Optional `page_ranges` annotations use `file`, `start_page`, and `end_page`; optional `exact_text` separates a literal identifier from natural-language query wording.
- **Order:** all required visual moments must be present in increasing timestamp order. Reverse-order and same-frame controls must not pass.

CLI/MCP hits now include an optional `evidence` array, with `source`, `text`, `seconds`, `page`, and `line`. Sources include `video_frame`, `video_ocr`, `audio`, `image`, `image_ocr`, and `text`. The primary hit's timestamp remains the preview target. Combined queries expose up to three supporting moments. Old responses without this field can still be scored using their primary hit; they cannot prove multiple moments.

The expanded synthetic corpus covers identifiers, near-duplicate document conditions, RTF extraction and temporal controls. It has **no real speech**; add separately licensed noisy recordings, audiovisual conjunctions, long videos, multiple languages, and human-judged hard negatives before drawing broader conclusions. Do not compare products using different category filters, interfaces, corpus coverage, or weak-result thresholds without disclosing those differences.

## Beta retrieval behavior and limits

Video frame OCR follows the existing optional OCR switch, engine, languages, video interval and frame deduplication. OCR failures are reported as warnings while visual frames remain indexable. Update affected Full sources to refresh video/RTF extraction; changed fingerprints retain old completed content until its replacement finishes.

Explicit video queries using `then`/`followed by` (or supported Chinese order markers) split into at most three visual clauses. `while` or supported Chinese audio clauses split visual/audio evidence. Each clause searches only its relevant modality, with at most four candidate moments per file; reranking favors the weakest-supported condition. Candidate moments must score at least 0.35 and remain within 0.12 of that file's strongest moment for the clause; these are conservative heuristic cutoffs, not probabilities. Ordered clauses require distinct increasing timestamps. Visual/audio evidence must be within 30 seconds. This is a bounded heuristic, not a general language parser or verified scene reasoning. It may increase latency by up to three additional query embeddings. Ordinary queries retain a single embedding request.

Brief scenes, tiny/obscured text, unsupported phrasing, and frame deduplication can still limit recall. Similarity-based evidence is labelled as a candidate; it must not be presented as proof that all requested facts are true.

An ignored Rust runner, `synthetic_retrieval_benchmark_records_real_backend_results`, can index and query an explicitly supplied isolated profile (`VECTORDASH_BENCHMARK_HOME`). It requires `synthetic-benchmark.flag`, `settings.json`, and `ground-truth.json` in that profile, and writes `results.jsonl`. It uses category-scoped searches; score those runs with `--category-scoped`. Never point it at your ordinary profile.

## Local validation, 2026-11-10

On the generated 156-file/288-query corpus, the beta returned all 288 expected files first, with all 168 annotated evidence checks passing (24 timestamp/modality cases, 156 exact-text cases, 12 order cases; these overlap). The corpus was used during development to catch FFmpeg frame alignment and surrounding-punctuation mismatches. It is **not a held-out benchmark**, and contains no real audio or page-annotated PDFs. These numbers establish regression coverage, not general accuracy or a comparison with other products.
