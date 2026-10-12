# Performance measurements — v0.2.22

Measured on October 11, 2026, on Windows 11 with a WD_BLACK SN850X NVMe drive. These are development measurements, not a speed guarantee.

## What was tested

- Isolated encrypted and unencrypted copies of the same existing catalog: **988,658 files** (985,149 Quick, 3,509 Full), plus 133,700 folders and 34,678 768-dimensional vectors. The encrypted database was about 1.92 GiB. Original sources and settings were preserved.
- Three performance profiles, each with encryption on and off. Search modes: Names, Semantic, and Both. Queries cover short prefixes, broad/exact names, photos, screenshots, documents and audio.
- The same already-loaded local EmbeddingGemma 2 server was used throughout. Timings include query embedding where applicable. Model startup/loading is excluded.
- Windows file caches were **not flushed**; these are not cold-boot disk benchmarks. “First” means application result/query/vector caches were cleared. “Warm retrieval” is the median of three fresh retrievals/query embeddings with the vector cache retained. “Repeat” is the median of three cached searches.
- Backend timings exclude GUI work. UI timings separately include debounce, retrieval, result cards and selected preview. RAM figures are process RSS, excluding the separate model/server and parser workers.

## Profiles and limits

| Profile | Database pages per search connection | Total compact vector cache | Typing delay |
| --- | ---: | ---: | ---: |
| Low memory | 8 MiB | None; streamed | 150 ms |
| Balanced (default) | 32 MiB | Up to 128 MiB | 75 ms |
| Fast search | 64 MiB | Up to 512 MiB | 35 ms |

Limits are caps, not reserved memory. The filename catalog and database remain on disk. Vector caches store float32 vectors, IDs and compact path/kind/channel metadata, not document text or the million-file filename catalog. Matrices exceeding the selected budget stream from disk. Result caching is limited to 32 queries / 4 MiB. Indexing uses the same small database caches across all profiles.

## Backend search: every profile and encryption setting

All times below are milliseconds. Query labels: short prefix = `f`; broad name / combined = `forza`; exact name = `forzahorizon6.exe`; photo meaning / combined = `images of dogs`; screenshot meaning = `server room`; document words / combined = `invoice`; audio meaning = `where they talk about sleep`. These measure retrieval latency, not an audited relevance score.

| Encryption | Profile | Mode / query | First | Warm retrieval | Repeat |
| --- | --- | --- | ---: | ---: | ---: |
| On | Low memory | Names: short prefix | 10.6 | 8.4 | 0.5 |
| On | Low memory | Names: broad name | 13.9 | 13.6 | 0.5 |
| On | Low memory | Names: exact name | 6.2 | 6.8 | 0.1 |
| On | Low memory | Semantic: photo meaning | 661.6 | 379.7 | 0.4 |
| On | Low memory | Semantic: screenshot meaning | 628.4 | 605.0 | 1.3 |
| On | Low memory | Semantic: document words | 552.9 | 558.2 | 0.9 |
| On | Low memory | Semantic: audio meaning | 392.1 | 338.4 | 0.5 |
| On | Low memory | Both: broad combined | 585.4 | 565.1 | 0.8 |
| On | Low memory | Both: photo combined | 365.3 | 372.8 | 0.6 |
| On | Low memory | Both: document combined | 573.0 | 545.6 | 1.1 |
| On | Balanced | Names: short prefix | 10.3 | 9.0 | 0.5 |
| On | Balanced | Names: broad name | 13.8 | 14.0 | 0.5 |
| On | Balanced | Names: exact name | 6.0 | 6.4 | 0.0 |
| On | Balanced | Semantic: photo meaning | 483.7 | 83.0 | 0.3 |
| On | Balanced | Semantic: screenshot meaning | 586.0 | 238.3 | 1.2 |
| On | Balanced | Semantic: document words | 531.3 | 182.1 | 1.1 |
| On | Balanced | Semantic: audio meaning | 411.2 | 64.1 | 0.6 |
| On | Balanced | Both: broad combined | 570.6 | 194.5 | 0.9 |
| On | Balanced | Both: photo combined | 462.3 | 81.3 | 0.4 |
| On | Balanced | Both: document combined | 543.2 | 191.6 | 1.1 |
| On | Fast search | Names: short prefix | 9.9 | 8.4 | 0.5 |
| On | Fast search | Names: broad name | 13.5 | 13.2 | 0.4 |
| On | Fast search | Names: exact name | 6.0 | 6.1 | 0.0 |
| On | Fast search | Semantic: photo meaning | 478.4 | 95.8 | 0.4 |
| On | Fast search | Semantic: screenshot meaning | 637.3 | 230.7 | 1.3 |
| On | Fast search | Semantic: document words | 566.3 | 194.5 | 1.0 |
| On | Fast search | Semantic: audio meaning | 416.9 | 41.3 | 0.5 |
| On | Fast search | Both: broad combined | 557.1 | 194.2 | 0.8 |
| On | Fast search | Both: photo combined | 460.6 | 96.7 | 0.3 |
| On | Fast search | Both: document combined | 572.7 | 184.9 | 1.0 |
| Off | Low memory | Names: short prefix | 8.8 | 7.5 | 0.5 |
| Off | Low memory | Names: broad name | 10.9 | 10.5 | 0.5 |
| Off | Low memory | Names: exact name | 3.1 | 3.0 | 0.0 |
| Off | Low memory | Semantic: photo meaning | 165.3 | 152.7 | 0.3 |
| Off | Low memory | Semantic: screenshot meaning | 334.5 | 324.7 | 1.3 |
| Off | Low memory | Semantic: document words | 282.8 | 316.6 | 1.0 |
| Off | Low memory | Semantic: audio meaning | 147.1 | 128.2 | 0.5 |
| Off | Low memory | Both: broad combined | 287.0 | 322.3 | 0.8 |
| Off | Low memory | Both: photo combined | 160.8 | 154.6 | 0.4 |
| Off | Low memory | Both: document combined | 312.9 | 286.4 | 1.0 |
| Off | Balanced | Names: short prefix | 8.7 | 7.4 | 0.5 |
| Off | Balanced | Names: broad name | 11.0 | 10.2 | 0.5 |
| Off | Balanced | Names: exact name | 2.8 | 2.8 | 0.0 |
| Off | Balanced | Semantic: photo meaning | 261.3 | 71.6 | 0.4 |
| Off | Balanced | Semantic: screenshot meaning | 356.8 | 189.4 | 1.3 |
| Off | Balanced | Semantic: document words | 306.3 | 156.0 | 1.0 |
| Off | Balanced | Semantic: audio meaning | 198.9 | 45.1 | 0.6 |
| Off | Balanced | Both: broad combined | 329.5 | 165.3 | 0.7 |
| Off | Balanced | Both: photo combined | 259.8 | 71.3 | 0.3 |
| Off | Balanced | Both: document combined | 319.5 | 148.9 | 1.1 |
| Off | Fast search | Names: short prefix | 8.4 | 7.0 | 0.5 |
| Off | Fast search | Names: broad name | 12.0 | 10.7 | 0.5 |
| Off | Fast search | Names: exact name | 2.8 | 2.6 | 0.0 |
| Off | Fast search | Semantic: photo meaning | 258.7 | 69.8 | 0.6 |
| Off | Fast search | Semantic: screenshot meaning | 358.6 | 186.0 | 1.2 |
| Off | Fast search | Semantic: document words | 333.5 | 156.7 | 1.2 |
| Off | Fast search | Semantic: audio meaning | 222.8 | 47.7 | 0.5 |
| Off | Fast search | Both: broad combined | 333.8 | 142.4 | 0.7 |
| Off | Fast search | Both: photo combined | 250.6 | 75.7 | 0.3 |
| Off | Fast search | Both: document combined | 327.3 | 139.5 | 1.0 |

| Encryption | Profile | Peak backend RAM | Retained vector cache |
| --- | --- | ---: | ---: |
| On | Low memory | 103.1 MiB | 0.0 MiB |
| On | Balanced | 231.2 MiB | 103.1 MiB |
| On | Fast search | 267.3 MiB | 103.1 MiB |
| Off | Low memory | 103.4 MiB | 0.0 MiB |
| Off | Balanced | 230.8 MiB | 103.1 MiB |
| Off | Fast search | 266.8 MiB | 103.1 MiB |

Balanced fits this catalog’s approximately 103 MiB compact vector cache. Fast search therefore does not deliver a consistently faster backend here; it mainly offers a larger limit for larger semantic catalogs and a shorter typing delay. Encryption has a larger effect in Low memory mode because it repeatedly reads/decrypts vector pages.

## Main app and popup: typed query to visible results

One measured first display per case and one repeated display per view/configuration; these are single UI observations rather than percentile statistics. Images may need disk decoding on their first preview, so first-display variation can be larger than backend variation. Repeats retain existing result cards. All values include the configured typing delay.

| Encryption | Profile | View | Names `forza` | Semantic `images of dogs` | Both `invoice` | Repeat range | Peak full-app RAM |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| On | Low memory | Main | 290.9 ms | 669.3 ms | 831.8 ms | 3.0–8.9 ms | 227.2 MiB |
| On | Low memory | Popup | 296.6 ms | 617.0 ms | 844.5 ms | 4.7–6.1 ms | 227.2 MiB |
| On | Balanced | Main | 221.6 ms | 268.7 ms | 406.9 ms | 3.7–7.1 ms | 336.0 MiB |
| On | Balanced | Popup | 229.1 ms | 254.7 ms | 395.0 ms | 4.1–8.1 ms | 336.0 MiB |
| On | Fast search | Main | 193.9 ms | 234.1 ms | 344.3 ms | 4.1–7.8 ms | 331.2 MiB |
| On | Fast search | Popup | 206.8 ms | 232.3 ms | 335.7 ms | 4.2–6.0 ms | 331.2 MiB |
| Off | Low memory | Main | 297.3 ms | 1070.1 ms | 604.8 ms | 4.5–7.0 ms | 227.6 MiB |
| Off | Low memory | Popup | 315.8 ms | 389.8 ms | 575.6 ms | 4.0–6.9 ms | 227.6 MiB |
| Off | Balanced | Main | 215.3 ms | 929.1 ms | 393.9 ms | 2.9–4.5 ms | 349.6 MiB |
| Off | Balanced | Popup | 222.7 ms | 212.5 ms | 339.2 ms | 3.6–8.2 ms | 349.6 MiB |
| Off | Fast search | Main | 188.6 ms | 908.4 ms | 342.5 ms | 4.9–7.4 ms | 343.1 MiB |
| Off | Fast search | Popup | 191.1 ms | 186.0 ms | 321.3 ms | 4.5–6.9 ms | 343.1 MiB |

## Mixed Quick / Full indexing

Generated directory entries exercise catalog enumeration/planning and database updates on the same NVMe. Each run contains 100,000 Quick entries plus 3,500 real text fixtures in a nested Full source. The update changes 343 Quick entries and deletes one Full file. A separate run uses 1,000,000 Quick entries. Text extraction is included; model inference, real drive traversal, expensive media parsers and GUI memory are excluded. This specifically checks the mixed-source retained-file query that previously stalled.

| Encryption | Profile | Quick entries | Initial index | Update | Peak backend RAM |
| --- | --- | ---: | ---: | ---: | ---: |
| On | Low memory | 100,000 | 12.3 s | 4.17 s | 130.3 MiB |
| On | Balanced | 100,000 | 12.22 s | 4.19 s | 129.8 MiB |
| On | Fast search | 100,000 | 12.23 s | 4.27 s | 129.4 MiB |
| Off | Low memory | 100,000 | 11.38 s | 3.06 s | 128.4 MiB |
| Off | Balanced | 100,000 | 11.11 s | 3.05 s | 129.3 MiB |
| Off | Fast search | 100,000 | 11.54 s | 3.08 s | 128.9 MiB |
| On | Balanced | 1,000,000 | 65.49 s | 38.29 s | 132.1 MiB |

All completed updates report **343 indexed files**, reuse the remaining unchanged files, remove the deleted document, and retain 3,499 Full text segments. Source counts include nested sources; recent additions count newly discovered paths rather than modifications. Old indexes have no historical addition timestamps, so their per-source activity begins with the next successful update.

## What changed

- Retained Full-file validation now uses indexed path lookups across mixed Quick/Full catalogs, with cancellation checks. The previous missing path index could turn this phase into repeated million-row scans.
- Compiled source/exclusion rules replace repeated path construction and rule normalization per search candidate.
- Name search has an indexed prefix lane and a bounded substring candidate pool. One/two-character queries use filename prefixes; longer queries also match substrings. Broad queries retrieve bounded candidates rather than materializing every match.
- Semantic search can use explicitly bounded compact vector caching, with a streaming fallback. Quick-only updates retain valid vector caches.
- Only the visible search window builds result cards/previews. Repeated identical results reuse their cards. A small result cache invalidates when settings or the index change.

A larger or colder index, slower model/hardware, deep source enumeration, antivirus activity or complex files can take longer. Encryption off is a performance option with a privacy cost, not a requirement for fast search. Disabling it makes stored filenames, text and vectors readable to anyone with access to the database.
