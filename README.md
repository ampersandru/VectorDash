# VectorDash

**A local, universal search, clipboard manager, and launcher for Windows 11 and Windows 10 (BETA). Powered by Google's EmbeddingGemma-2.**

VectorDash is meant to be a **full Windows search replacement**: one search bar for files/folders, installed apps, Windows settings, and quick calculations. Press **Ctrl+Alt+Space** (configurable) from any app to find what you need.

Search by name or exact words, or describe what you remember. Google's **EmbeddingGemma 2** powers semantic file and media search. Filename search, app launching, settings shortcuts and the calculator work without a model.

Includes a clipboard manager, which lets you store copied text and images for a configurable amount of time. Option to enable semantic search to use EmbeddingGemma-2 to search copied images for exactly what copied in the past!

<img width="996" height="690" alt="20261014-2243-51 0176601" src="https://github.com/user-attachments/assets/2790bc54-4d5c-451a-8117-2259662a7f8f" />


## What you can search

| Search for | Example | What happens |
| --- | --- | --- |
| **Files and folders** | `invoice`, `forzahorizon6.exe` | Find names and paths across your selected drives and folders. |
| **File contents** | `a dog on the beach`, `the clause about pets` | Find matching photos, screenshots, document passages, PDF pages, or audio/video moments. |
| **Apps** | `chrome`, `app: calculator` | Launch Start menu programs and Windows apps, with their actual icons in results and previews. |
| **Windows settings** | `bluetooth`, `settings: sound` | Open settings pages such as Wi-Fi, display, storage, sound and Windows Update. |
| **Calculations** | `24*1.2`, `20% of 150`, `sqrt(81)` | Get an instant local answer; press **Enter** to copy it. |

- **Indexing controls:** choose Quick or Full first, prepare one batch ahead and share long-file work fairly; optional near-identical frame skipping and learned timing estimates are BETA and need broader media testing.
- **File search:** combine names, exact words and semantic matches. Read PDF, Word, Excel and PowerPoint content; optionally use OCR for screenshot text. Search across 100+ languages and opt into code search.
- **Quick search and previews:** Hotkey quicksearch from anywhere! Preview results, inspect why they matched, and open, copy or reveal files in Explorer.
- **Your sources, your choice:** index names only for fast drive-wide search, or fully index selected folders. Nested Full folders override Quick drives. Select multiple search sources, exclude files/types or common Windows cache folders, and see Full/Quick counts and recent additions.
- **Advanced and saved searches:** combine filenames, wildcards, size and dates, such as name:dog*.jpg size:>1MB or ext:pdf modified:today. Save queries together with their modes, sources and exclusions; rename or remove them in Settings.
- **Automatic updates:** monitor new, modified and removed files, or schedule scans. Automatic changes obey CPU/GPU, inactivity and power conditions even when periodic scanning is Manual. Optional NTFS journal tracking replays saved changes when volume access is available; folder notifications and recovery scans provide fallback.
- **Flexible setup:** choose GPU, iGPU or CPU/system memory; install selected optional features or remove downloaded tools. Status shows disk usage and live process RAM/VRAM. Local models warm when Hybrid/Semantic search opens, with loading progress, and can unload after an adjustable idle period. Optional game/full-screen rules pause indexing and unload models; Load anyway overrides model limits for that search session. Use **Models and AI** for embedding, dictation, LLM and voice models. Remote checks report readiness, vision/mmproj support, output dimensions and available context; manual context limits stay editable. **Hardware** shows combined local CPU/GPU, memory and storage usage. Media search requires a compatible multimodal embedding server.
- **Web search:** optionally connect your own SearXNG or Degoog instance, or Brave Search API. Choose a provider in the search bar and press Enter; open results in your default browser. Web search uses no embedding model and is off by default. Configure providers in **Web** settings; reorder search mode buttons and keyboard cycling in **Settings → Search and dictation**.
- **Emoji and clipboard:** an offline full-color emoji grid and optional clipboard history for text, links and images, with timestamps, previews and expiry. Optionally search copied images by meaning with EmbeddingGemma 2, just like regular search.
- 
<img width="1006" height="714" alt="20261015-0700-29 5512708" src="https://github.com/user-attachments/assets/ef656421-b0b0-4842-85b5-9d5266dac6c6" />

- **Voice dictation:** optional local Parakeet speech recognition on CPU or NVIDIA GPU, or a remote OpenAI-compatible transcription server. The model loads immediately on activation and unloads immediately or after your chosen idle interval. Set a voice-shortcut popup default (Search, Clipboard or AI), response length and automatic read aloud in Settings.
- **Make it yours:** light/dark themes, app accent colors, popup tint/transparency/gradient, a collapsible icon sidebar. About includes updates and a changelog covering releases since your installed version.
- **Camera RAW (BETA):** optional ARW, CR2/CR3, DNG, RAF, NEF and other LibRaw-supported formats use embedded previews first, with optional development fallback; enable in Sources, save and update a Full source, and please test your camera files.
- **Interface localization (BETA):** 13 languages cover search, settings, help, dialogs and tray menus; native-speaker and complex-script testing is still needed.
- **Indexing reliability:** file notifications stay targeted, interrupted chunks resume, inaccessible network subfolders do not stop accessible siblings, and completed manual scans satisfy older recovery work. Notification overflows retain their source and use healthy NTFS journal replay; unreadable journal parents use targeted filename checks rather than drive-wide rescans. Deleted-file cleanup runs in small batches and reports its own removed-record count and elapsed time.
- **Search tuning (BETA):** optional FP16 semantic caching reduces vector RAM, while Rank Calibration adjusts cross-media ranking at search time without changing stored embeddings or requiring reindexing; test relevance on your own collection.
- **Resizable previews and picture grids:** Search and Clipboard dividers remember their widths, Images/Videos grids have a separate preview width, and video thumbnails load on demand into a bounded RAM cache.
- **Update channels:** choose Release or Beta in About, inspect changes since your installed version, and return to VectorDash automatically after a UI update.

<img width="1004" height="696" alt="20261014-2254-19 0296609" src="https://github.com/user-attachments/assets/5d5a8351-277e-4a29-9087-22e537d5398b" />


### AI and agent access

Switch to **AI** to ask questions using your own local or remote **OpenAI-compatible server**. In **Models and AI**, enter a base URL such as `http://localhost:8080/v1`, an optional API key, and your model; refresh to select from the server's models and set a maximum context size.

Enable **AI chat** and select a model to show the AI tab. Choose **Brief** (one or two direct, spoken-style sentences - just like a voice assistant), **Medium** (8–12 sentences) or **Full** (model default).

With a vision-capable model, attach a monitor, window or selected screen area to ask about what you see.

The optional **MCP server**, on the same settings page, lets compatible agents search indexed files and retrieve file information. Keep it local or enable access on your LAN, with optional bearer authentication. See [MCP setup](docs/MCP.md) for connection details.

<img width="982" height="682" alt="20261015-0425-02 7332021" src="https://github.com/user-attachments/assets/2d7ff9ab-b74e-434f-93d0-688ff0e15d14" />

## Install

1. Download the latest **Windows x64 setup EXE** from [Releases](https://github.com/ampersandru/VectorDash/releases) (about **106 MB**), or the **portable ZIP**.
2. Run the installer. **The EXE is not code-signed**, so Windows may show a SmartScreen or unknown-publisher warning. If you trust the release, choose **More info → Run anyway**, when available, to continue.
3. Choose CPU/GPU settings in **Hardware**, then open **Models and AI → Embedding Model → Download/setup** to install the model, encoder and selected runtime. Click **Start** when needed. **Q8_0** is the default. For names-only search, skip model setup and select Quick mode in Sources.
4. Add folders or drives in **Sources**, then click **Update index**. Start with a small folder to check results.

**Clear an index:** use **Sources → Clear entire index** and confirm. This removes indexed names, extracted text and vectors, while keeping your original files, sources, settings and downloaded models. Automatic indexing stays paused until you resume it or click Update index. Previously retained import/conversion backups are separate and are not deleted.

Enable **Include Start menu programs** in Sources to search installed program shortcuts. The catalog loads at startup. Select a result and press **Enter** to launch its original shortcut, preserving arguments and working folders.

**File/Folder Names** search needs no model. In **Sources**, choose **Quick file / folder names** or **Full index** before adding drives, folders or files. In either search window, choose **Hybrid**, **File/Folder Names** or **Semantic**, and use the source dropdown to search some or all sources.

| Indexing mode | What it indexes | Uses EmbeddingGemma? | Recommended use |
| --- | --- | --- | --- |
| **Quick** | File and folder names and paths; no file-content extraction or semantic embeddings. Faster indexing with lower resource use. | **No.** Quick indexing and filename searches work without an embedding model. | General files and folders, entire drives, and anything you mainly want to find by name or location. |
| **Full** | Names and paths, plus supported document contents and image, video and audio content according to your indexing settings. Takes more time and storage. | **Yes, by default.** Uses local EmbeddingGemma 2 or your configured remote embedding model to create semantic embeddings. | Images, videos, documents, audio and other supported files whose contents or meaning you want to search later. |

**Search by folder path:** type `appdata\local` to find Local folders inside AppData, `\Local\RHI` to find RHI folders inside Local, or `\RHI\nvngx_dlssnr.dll` to find that file anywhere beneath an RHI folder. Matching ignores case, accepts `/` or `\`, and supports partial final names. Works with existing Quick and Full indexes without reindexing or an embedding model. Use `*nr*` for filename wildcards or `*\*nr*` for path wildcards. Quick sources can catalog names in AppData. **Exclude common Windows/cache folders** applies to both Quick and Full sources: saving it hides matching results and removes existing index records in the background. Other AppData names remain searchable in Quick sources. Explicit exclusions still apply. Run **Update index** after adding a source to make its files searchable.

**Recommended setup:** use Quick for broad drive coverage, then add specific media or document folders as Full sources. A more specific Full folder takes precedence over its parent Quick source. Exclusions and content-type settings still apply; unsupported files remain searchable by name.

**At search time:** File/Folder Names search needs no model. Semantic search, and the semantic part of Hybrid search, need the configured embedding model running locally or remotely. Selecting Hybrid does not give Quick-indexed files content-based matches; use Full indexing for those. Local image/audio embedding also needs its matching encoder. You can select existing model files instead of downloading them. Switching Local/Remote, server addresses or model file paths preserves the index. Use the same compatible model, dimensions, pooling and preprocessing on both backends; use **Rebuild full content** explicitly when changing to an incompatible model or encoder.

| Shortcut | Action |
| --- | --- |
| **Ctrl+Alt+Space** | Open quick search; configurable in Settings |
| **Ctrl+Shift+D** | Open quick search and dictate, or start/finish dictation in the current search |
| **Space** | Open/close the larger preview |
| **Enter** | Open a file, launch an app, open settings, or copy a calculator answer |
| **Ctrl+C** | Copy the selected file, calculator answer or settings link |
| **Tab / Shift+Tab** | Cycle enabled Search, Clipboard and AI modes forward/backward. Enter submits an AI question |
| **Ctrl+Tab** | Cycle file types; configurable in Settings. The current type appears inside the search field |
| **↑ / ↓** | Select results and scroll the list |
| **Ctrl+F** | Return to search from settings |
| **Double Ctrl** | Open quick search in Explorer or a standard Open/Save dialog when enabled |

Customize hotkeys in **Settings → Search and dictation → Keyboard Shortcut**.

Windows 10 support is **BETA**, targeting x64 version 2004/build 19041 or newer; Windows 11 remains the primary tested platform, and actual Windows 10 device testing is still needed.

## Models and tools

Approximate sizes vary by release. EmbeddingGemma sizes are individual files; Parakeet and runtime sizes are compressed downloads. Installed files, the index and RAM/VRAM use require additional space.

| [EmbeddingGemma 2](https://huggingface.co/ggml-org/embeddinggemma-2-GGUF) | Text model | Matching image/audio encoder (mmproj) |
| --- | --- | --- |
| **Q8_0** — default | 310 MB | 555 MB |
| **BF16** | 558 MB | 982 MB |

| [Parakeet TDT 0.6B v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) (Optional) | Model download | With CPU runtime | With CUDA runtime/libraries |
| --- | --- | --- | --- |
| **8-bit** | 482 MB | ~500 MB total | ~2.31 GB total |
| **fp16** | 1.12 GB | ~1.14 GB total | ~2.95 GB total |

Parakeet provides **English speech-to-text**, independently of semantic search. Either model can use CPU or NVIDIA CUDA. Models load on microphone activation; immediate unloading is the default. Installed disk usage is shown in Status.

| Tool | What it does | Approx. size |
| --- | --- | --- |
| [llama.cpp / llama-server](https://github.com/ggml-org/llama.cpp) — CPU | Runs the model using system memory | 20 MB |
| llama-server — Vulkan | GPU/iGPU inference with compatible drivers | 32 MB |
| llama-server — CUDA | NVIDIA GPU inference, including companion runtime | 577–657 MB |
| [Tesseract OCR](https://github.com/UB-Mannheim/tesseract/wiki) — optional | Reads exact words in screenshots and scanned PDFs | 50 MB |
| [OCR language packs](https://github.com/tesseract-ocr/tessdata_fast) — optional | Enables OCR for selected languages | English 4 MB |
| Parakeet speech runtime — optional | Runs the dictation model above | CPU ~19 MB; CUDA and libraries ~1.83 GB |
| [FFmpeg / FFprobe / FFplay](https://ffmpeg.org/) | Compatibility video decoder, audio extraction and matched-moment playback; bundled | About 150 MB unpacked |
| [LibRaw](https://www.libraw.org/) — camera RAW (BETA) | Isolated embedded-preview decoding and optional development fallback; bundled, disabled by default | Decoder/runtime ~3 MB, plus source and license notices |
| [PDFium](https://pdfium.googlesource.com/pdfium/) | PDF extraction and page previews; bundled | About 10–25 MB |

Install only the runtime for your hardware. OCR is optional for image meaning searches; it helps with exact text. Windows native decoding is the default for video frames, with separate CPU/GPU settings and optional FFmpeg fallback. FFmpeg is still used for audio extraction/playback; semantic media search also needs the matching encoder.

**Download trouble?** Setup caches official catalogs and retries temporary failures. Rate-limit messages identify GitHub (runtimes) or Hugging Face (models) and suggest when to retry. Optional **GitHub / Hugging Face download tokens** are in **Hardware → Optional download tokens**; save them before retrying. They are protected by your Windows account and sent only to their own service. Interrupted downloads retain partial files for resume, and checksums are still required. You can also select an existing runtime/model manually.


## Performance

The database and filename catalog stay on disk. Background indexing uses bounded memory and reuses unchanged files. Each source shows its indexed file count and new additions in the past hour.

Choose **Settings → Search performance**:

| Profile | Search cache limits | Best for |
| --- | --- | --- |
| **Low memory** | 8 MiB of database pages per search; vectors streamed from disk | Lowest RAM use |
| **Balanced** — default | 32 MiB of pages per search; up to 128 MiB of compact vectors total | Fast everyday search |
| **Fast search** | 64 MiB of pages per search; up to 512 MiB of compact vectors total | Larger semantic indexes |

On a development NVMe test with **1,076,786 files** and **14,123 vectors**, encrypted Balanced searches measured about **9 ms for warm filename searches**, **32 ms for warm semantic retrieval**, and **47 ms for Hybrid**. These are backend timings with warm disk caches and an already loaded GPU model; cold model loading, UI delay and hardware add time. An empty native GUI measured about **115 MiB**, excluding model/helper memory. See [all native profiles, encryption on/off, indexing measurements and limitations](native/docs/BENCHMARKS.md).

**Database encryption** is enabled by default. Settings lets you change it and explains the performance tradeoff. Encryption protects stored filenames, extracted text and embeddings but adds processing overhead. Changing it creates a converted database copy without re-embedding; the original is retained, including any plaintext copy. Portable encrypted data is tied to your Windows account/machine. Existing Rust installations retain their settings/index; older Python users can import a copy through Sources.

## Privacy and limitations

Local mode keeps indexed content on your computer; setup downloads need internet access. A remote model server receives the content sent for embedding. Enable LAN/MCP access only for trusted clients. See [security details](docs/SECURITY.md).

AI search can miss or misrank results. OCR can misread text, and sampled video/audio matches give approximate moments. Check original files before relying on a result. VectorDash was developed with AI assistance.

[File support](docs/RETRIEVAL.md) · [MCP setup](docs/MCP.md) · [Build from source and migration](native/docs/MIGRATION.md)

VectorDash is an independent Windows project inspired by [DigUp](https://github.com/ARahim3/DigUp), created by Abdur Rahim. See [acknowledgements](ACKNOWLEDGEMENTS.md). Licensed under [MIT](LICENSE).
