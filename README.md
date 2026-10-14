# VectorDash

**A local, universal search and launcher for Windows 11. Built in Rust.**

VectorDash is meant to be a **full Windows search replacement**: one search bar for files and folders, installed apps, Windows settings, and quick calculations. Press **Ctrl+Alt+Space** from any app to find what you need.

Search by name or exact words, or describe what you remember. Google's **EmbeddingGemma 2** powers semantic file and media search. Filename search, app launching, settings shortcuts and the calculator work without a model.

<img width="1022" height="662" alt="20261013-0722-00 1967232" src="https://github.com/user-attachments/assets/81de993d-7d66-4a9b-b335-7d7cad1b0633" />


## What you can search

| Search for | Example | What happens |
| --- | --- | --- |
| **Files and folders** | `invoice`, `forzahorizon6.exe` | Find names and paths across your selected drives and folders. |
| **File contents** | `a dog on the beach`, `the clause about pets` | Find matching photos, screenshots, document passages, PDF pages, or audio/video moments. |
| **Apps** | `chrome`, `app: calculator` | Launch Start menu programs and Windows apps, with their actual icons in results and previews. |
| **Windows settings** | `bluetooth`, `settings: sound` | Open settings pages such as Wi-Fi, display, storage, sound and Windows Update. |
| **Calculations** | `24*1.2`, `20% of 150`, `sqrt(81)` | Get an instant local answer; press **Enter** to copy it. |

- **File search:** combine names, exact words and semantic matches. Read PDF, Word, Excel and PowerPoint content; optionally use OCR for screenshot text. Search across 100+ languages and opt into code search.
- **Quick search and previews:** a frosted popup with adjustable color, transparency and gradient. It stays compact until you type. Preview results, inspect why they matched, and open, copy or reveal files in Explorer.
- **Your sources, your choice:** index names only for fast drive-wide search, or fully index selected folders. Nested Full folders override Quick drives. Select multiple search sources, exclude files/types or common Windows cache folders, and see Full/Quick counts and recent additions.
- **Advanced and saved searches:** combine filenames, wildcards, size and dates, such as name:dog*.jpg size:>1MB or ext:pdf modified:today. Save queries together with their modes, sources and exclusions; rename or remove them in Settings.
- **Automatic updates:** monitor new, modified and removed files, or schedule scans. Automatic changes obey CPU/GPU, inactivity and power conditions even when periodic scanning is Manual. Optional NTFS journal tracking replays saved changes when volume access is available; folder notifications and recovery scans provide fallback. Pause/cancel indexing and inspect VectorDash, model and File logs.
- **Flexible setup:** choose GPU, iGPU or CPU/system memory; install selected optional features or remove downloaded tools. Status shows disk usage and live process RAM/VRAM. Local models warm when Hybrid/Semantic search opens, with loading progress, and can unload after an adjustable idle period. Optional game/full-screen rules pause indexing and unload models; Load anyway overrides model limits for that search session. Select Local or Remote for the embedding server; remote setup includes model discovery, a context budget and an embeddings check. Media search requires a compatible multimodal embedding server.
- **Emoji and clipboard:** an offline full-color emoji grid and optional clipboard history for text, links and images, with timestamps, previews and expiry. Optionally search copied images by meaning with EmbeddingGemma 2.
- **Voice dictation:** optional local Parakeet speech recognition on CPU or NVIDIA GPU, or a remote OpenAI-compatible transcription server. Choose a microphone and fp16/8-bit model; watch the full-width waveform inside the search field and let silence finish recording. Trailing sentence punctuation is removed from transcriptions. A soft sound and a rotating red-to-orange trail indicate recording. The model loads immediately on activation and unloads immediately or after your chosen idle interval. Dictation stays in your active search mode; in AI mode it submits the transcription automatically. Set a voice-shortcut popup default (Search, Clipboard or AI), response length and automatic read aloud in Settings.
- **Make it yours:** light/dark themes, app accent colors, popup tint/transparency/gradient, a collapsible icon sidebar, remembered window positions, and one red Save / outlined Undo bar for settings. About includes updates and a changelog covering releases since your installed version.

<img width="1000" height="678" alt="20261013-0738-48 7475530" src="https://github.com/user-attachments/assets/1eabcb5e-63ae-47d6-872b-fa72d85b91cd" />

### AI and agent access

Switch to **AI** to ask questions using your own local or remote **OpenAI-compatible server**. In **AI settings**, enter a base URL such as `http://localhost:8080/v1`, an optional API key, and your model; refresh to select from the server's models and set a maximum context size.

Enable **AI chat** and select a model to show the AI tab; disabled AI and clipboard modes stay hidden and are skipped by Tab. Choose **Brief** (one or two direct, spoken-style sentences), **Medium** (8–12 sentences or two paragraphs) or **Full** (model default). Press **Enter** to send, ask follow-up questions, or **Stop** a request. A purple-to-blue trail indicates AI processing. Answers support Markdown, full-color emoji and scrolling; **Ctrl+C** or the reply’s Copy button copies the last answer. Follow-up questions scroll to the new response. Reported prompt/decode speeds appear when the server supplies them. With a vision-capable model, attach a monitor, window or selected screen area to ask about what you see. Sent screenshots appear as clickable thumbnails with a copy option. AI requests and screenshots go to your configured server. Enable or disable **Text to speech** in AI settings; disabled TTS hides read-aloud controls. Optional speech settings connect a separate OpenAI-compatible speech server: refresh models/voices, select a voice, and use the reply’s speaker button or **Read Aloud**. Supported speech servers stream audio as it arrives; other servers fall back to normal audio playback. Dictation pauses read aloud, and closing a search window stops it.

The optional **MCP server**, on the same settings page, lets compatible agents search indexed files and retrieve file information. Keep it local or enable access on your LAN, with optional bearer authentication. See [MCP setup](docs/MCP.md) for connection details. AI chat and MCP are optional; ordinary file search works independently.

<img width="1014" height="684" alt="20261014-0125-45 9282264" src="https://github.com/user-attachments/assets/a99b62d1-a713-4027-88d0-d3dd0604abeb" />


## Install

1. Download the latest **Windows x64 setup EXE** from [Releases](https://github.com/ampersandru/VectorDash/releases) (about **97 MB**), or the **portable ZIP**. Python is not required.
2. Run the installer. **The EXE is not code-signed**, so Windows may show a SmartScreen or unknown-publisher warning. If you trust the release, choose **More info → Run anyway**, when available, to continue.
3. **Optional semantic search:** open **Models & hardware → Status** to install features, then choose hardware and model. Click **Start** when needed. **Q8_0** is the default. For names-only search, skip model setup and select Quick mode in Sources. The calculator and Windows settings shortcuts are ready immediately.
4. Add folders or drives in **Sources**, then click **Update index**. Start with a small folder to check results.

**Upgrading:** setup installs as **VectorDash** and replaces the older Python installation in its existing folder. A separately registered Rust preview is removed after a successful upgrade. Settings, models and indexes are retained; older Python indexes can be imported as a copy through Sources.

**Clear an index:** use **Sources → Clear entire index** and confirm. This removes indexed names, extracted text and vectors, while keeping your original files, sources, settings and downloaded models. Automatic indexing stays paused until you resume it or click Update index. Previously retained import/conversion backups are separate and are not deleted.

Enable **Include Start menu programs** in Sources to search installed program shortcuts. The catalog loads at startup. Select a result and press **Enter** to launch its original shortcut, preserving arguments and working folders.

**File/Folder Names** search needs no model. In **Sources**, choose **Quick file / folder names** or **Full index** before adding drives, folders or files. To change existing sources, select entries (or **Select/Deselect all**) and apply a mode, then update the index. In either search window, choose **Hybrid**, **File/Folder Names** or **Semantic**, and use the source dropdown to search some or all sources. More specific folders take priority when sources overlap. Index status shows Full/Quick file counts and how many files have vectors. Enable **Skip Windows system and frequently changing cache folders** in Sources to omit temp files, crash dumps, update staging and selected caches while keeping personal folders and Program Files.

Semantic search needs the model and a compatible llama-server. Image/audio search also needs its matching encoder. You can select existing files instead of downloading them. Changing the model or encoder requires rebuilding the index once.

| Shortcut | Action |
| --- | --- |
| **Ctrl+Alt+Space** | Open quick search; configurable in Settings |
| **Ctrl+Shift+D** | Open quick search and dictate, or start/finish dictation in the current search |
| **Space** | Open/close the larger preview |
| **Enter** | Open a file, launch an app, open settings, or copy a calculator answer |
| **Ctrl+C** | Copy the selected file, calculator answer or settings link |
| **Tab / Shift+Tab** | Cycle enabled Search, Clipboard and AI modes forward/backward. Enter submits an AI question |
| **↑ / ↓** | Select results and scroll the list |
| **Ctrl+F** | Return to search from settings |
| **Double Ctrl** | Open quick search in Explorer or a standard Open/Save dialog when enabled |

## Models and tools

Approximate sizes vary by release. EmbeddingGemma sizes are individual files; Parakeet and runtime sizes are compressed downloads. Installed files, the index and RAM/VRAM use require additional space.

| [EmbeddingGemma 2](https://huggingface.co/ggml-org/embeddinggemma-2-GGUF) | Text model | Matching image/audio encoder (mmproj) |
| --- | --- | --- |
| **Q8_0** — default | 310 MB | 555 MB |
| **BF16** | 558 MB | 982 MB |

| Optional [Parakeet TDT 0.6B v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) | Model download | With CPU runtime | With CUDA runtime/libraries |
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
| [FFmpeg / FFprobe / FFplay](https://ffmpeg.org/) | Extracts media frames/audio and plays matched moments; bundled | About 150 MB unpacked |
| [PDFium](https://pdfium.googlesource.com/pdfium/) | PDF extraction and page previews; bundled | About 10–25 MB |

Install only the runtime for your hardware. OCR is optional for image meaning searches; it helps with exact text. FFmpeg and the encoder are needed for media processing.

**Download trouble?** Setup caches official catalogs and retries temporary failures. Rate-limit messages identify GitHub (runtimes) or Hugging Face (models) and suggest when to retry. Optional **GitHub / Hugging Face download tokens** are in **Models & hardware → Download and set up**; save them before retrying. They are protected by your Windows account and sent only to their own service. Interrupted downloads retain partial files for resume, and checksums are still required. You can also select an existing runtime/model manually.


## Performance

The database and filename catalog stay on disk. Background indexing uses bounded memory and reuses unchanged files. Each source shows its indexed file count and new additions in the past hour.

Choose **Settings → Search performance**:

| Profile | Search cache limits | Best for |
| --- | --- | --- |
| **Low memory** | 8 MiB of database pages per search; vectors streamed from disk | Lowest RAM use |
| **Balanced** — default | 32 MiB of pages per search; up to 128 MiB of compact vectors total | Fast everyday search |
| **Fast search** | 64 MiB of pages per search; up to 512 MiB of compact vectors total | Larger semantic indexes |

On a development NVMe test with **1,076,786 files** and **14,123 vectors**, encrypted Balanced searches measured about **9 ms for warm filename searches**, **32 ms for warm semantic retrieval**, and **47 ms for Hybrid**. These are backend timings with warm disk caches and an already loaded GPU model; cold model loading, UI delay and hardware add time. An empty native GUI measured about **115 MiB**, excluding model/helper memory. See [all native profiles, encryption on/off, indexing measurements and limitations](native/docs/BENCHMARKS.md).

Tray mode sleeps between background events instead of continuously repainting. Existing global shortcut choices are preserved across updates.

One- and two-character name searches use filename prefixes; longer queries also match substrings. Broad searches return a bounded selection of best candidates. Cache limits are caps, not reserved memory; models and active file-processing helpers use additional RAM/VRAM.

**Database encryption** is enabled by default. Settings lets you change it and explains the performance tradeoff. Encryption protects stored filenames, extracted text and embeddings but adds processing overhead. Changing it creates a converted database copy without re-embedding; the original is retained, including any plaintext copy. Portable encrypted data is tied to your Windows account/machine. Existing Rust installations retain their settings/index; older Python users can import a copy through Sources.

## Privacy and limitations

Local mode keeps indexed content on your computer; setup downloads need internet access. A remote model server receives the content sent for embedding. Enable LAN/MCP access only for trusted clients. See [security details](docs/SECURITY.md).

AI search can miss or misrank results. OCR can misread text, and sampled video/audio matches give approximate moments. Check original files before relying on a result. VectorDash was developed with AI assistance.

[File support](docs/RETRIEVAL.md) · [MCP setup](docs/MCP.md) · [Build from source and migration](native/docs/MIGRATION.md)

VectorDash is an independent Windows project inspired by [DigUp](https://github.com/ARahim3/DigUp), created by Abdur Rahim. See [acknowledgements](ACKNOWLEDGEMENTS.md). Licensed under [MIT](LICENSE).
