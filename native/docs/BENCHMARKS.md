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

18 native regression tests passed. Packaged AVIF and JPEG XL decoding fixtures passed. Audio streaming emits separate 30-second pieces; video frames retain their sampled times. A silent-video fixture caught an audio-absence failure, which was fixed by inspecting audio streams before extraction.

No matched Everything/Listary benchmark has been run; no parity claim is made. Compare identical folders and queries, cold/warm launch and search, result recall, memory including helpers, and initial/incremental indexing. Everything journal indexing and content embeddings are different workloads.

This is a migration preview: standard-dialog path insertion is implemented but every custom/elevated dialog has not been tested. Explorer activation opens quick search; it is not a shell-extension embedded results view. CPU/GPU-load scheduling, specialized XLSX cell reconstruction, legacy retrieval-evaluation parity and animated tray visuals need further work. Image/PDF GUI previews use native decoders in process; indexing parsers have separate low-integrity/job-limited workers. Retain the stable release until the native preview is validated for your workflows.
