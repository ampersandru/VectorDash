# VectorDash

**A local, universal search and launcher for Windows 11.**

VectorDash is meant to be a **full Windows search replacement**: one search bar for files and folders, installed apps, Windows settings, and quick calculations. Press **Ctrl+Shift+Space** from any app to find what you need.

Search by name or exact words, or describe what you remember. Google's **EmbeddingGemma 2** powers semantic file and media search. Filename search, app launching, settings shortcuts and the calculator work without a model.

<img width="1129" height="629" alt="image" src="https://github.com/user-attachments/assets/a3aa357e-a333-4d44-938e-f9cf3861ddc6" />

<img width="1120" height="626" alt="image" src="https://github.com/user-attachments/assets/439fd438-d4e0-477a-ba30-676509e6fb24" />

<img width="1193" height="881" alt="image" src="https://github.com/user-attachments/assets/7f431693-1b7a-4864-aecb-87b05c30c667" />


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
- **Your sources, your choice:** index names only for fast drive-wide search, or fully index selected folders. Filter types and sources, exclude files or common Windows cache folders, and schedule incremental updates.
- **Flexible setup:** choose GPU, iGPU or CPU/system memory. Optional remote llama-server and read-only [MCP access for agents](docs/MCP.md).

## Install

1. Download the latest **Windows x64 setup EXE** from [Releases](https://github.com/ampersandru/VectorDash/releases) (about **96 MB**). Python is included.
2. Run the installer. **The EXE is not code-signed**, so Windows may show a SmartScreen or unknown-publisher warning. If you trust the release, choose **More info → Run anyway**, when available, to continue.
3. **Optional semantic search:** open **Models & hardware → Status**, select features, hardware and model, then click **Download, install and start**. **Q8_0** is the default. For names-only search, skip model setup and select Quick mode in Sources. The calculator and Windows settings shortcuts are ready immediately.
4. Add folders or drives in **Sources**, then click **Update index**. Start with a small folder to check results.

To enable program launching, open **Sources → Start menu programs** and turn on **Include Start menu programs in search**. The catalog refreshes at startup; use **Scan Start menu** after installing a program. Select a result and press **Enter** to launch its original shortcut, preserving arguments and working folders.

**Names** search needs no model. In **Sources**, choose **Quick file / folder names** or **Full index** before adding drives, folders or files. To change existing sources, select entries (or **Select all**) and apply a mode, then update the index. In either search window, choose **Names**, **Semantic** or **Both**, and use the source dropdown to search some or all sources. More specific folders take priority when sources overlap. Index status shows Full/Quick file counts and how many files have vectors. Enable **Skip Windows system and frequently changing cache folders** in Sources to omit temp files, crash dumps, update staging and selected caches while keeping personal folders and Program Files.

Semantic search needs the model and a compatible llama-server. Image/audio search also needs its matching encoder. You can select existing files instead of downloading them. Changing the model or encoder requires rebuilding the index once.

| Shortcut | Action |
| --- | --- |
| **Ctrl+Shift+Space** | Open quick search; change this in Settings |
| **Space** | Open/close the larger preview |
| **Enter** | Open a file, launch an app, open settings, or copy a calculator answer |
| **Ctrl+C** | Copy the selected file, calculator answer or settings link |
| **Tab / Shift+Tab** | Cycle file categories |

## Models and tools

Approximate sizes vary by release. Model sizes are individual files; runtime sizes are compressed downloads. Installed files, the index and RAM/VRAM use require additional space.

| [EmbeddingGemma 2](https://huggingface.co/ggml-org/embeddinggemma-2-GGUF) | Text model | Matching image/audio encoder (mmproj) |
| --- | --- | --- |
| **Q8_0** — default | 310 MB | 555 MB |
| **BF16** | 558 MB | 982 MB |

| Tool | What it does | Approx. size |
| --- | --- | --- |
| [llama.cpp / llama-server](https://github.com/ggml-org/llama.cpp) — CPU | Runs the model using system memory | 20 MB |
| llama-server — Vulkan | GPU/iGPU inference with compatible drivers | 32 MB |
| llama-server — CUDA | NVIDIA GPU inference, including companion runtime | 577–657 MB |
| [Tesseract OCR](https://github.com/UB-Mannheim/tesseract/wiki) — optional | Reads exact words in screenshots and scanned PDFs | 50 MB |
| [OCR language packs](https://github.com/tesseract-ocr/tessdata_fast) — optional | Enables OCR for selected languages | English 4 MB |
| [FFmpeg](https://ffmpeg.org/) | Decodes audio/video and extracts frames and audio | 88 MB binary; no extra download |

Install only the runtime for your hardware. OCR is optional for image meaning searches; it helps with exact text. FFmpeg and the encoder are needed for media processing.

## Performance

The database, filenames and embedding index stay on disk. VectorDash uses bounded caches and background indexing so catalog size does not require keeping the whole index in RAM. Unchanged files are reused, and only changed name entries are updated. Models and active file-processing helpers use additional RAM/VRAM.

Development checks with **1,000,000 generated filename entries**:

| Check | Result |
| --- | --- |
| Filename search | 23–31 ms for 100 results |
| Backend peak memory during indexing | About 116 MB; about 77 MB afterward |
| Update with eight changed files | About 35 seconds to check the catalog and update it |

These checks exclude walking a real drive, model inference and drawing results. A separate full-app test during indexing used about 203 MB and kept the interface responding. Drive speed, file formats and model/hardware choice affect actual performance. Fully indexed semantic content is scored in bounded disk batches instead of a permanent whole-index RAM cache.

**Database encryption** is enabled by default. Change it in **Settings → Index privacy**, then restart. Encryption protects stored filenames, extracted text and embeddings but can add indexing/search overhead. Disabling it may improve disk performance; anyone with access to that database can read its contents. Existing indexes convert in the background on the first launch after upgrading or changing encryption. A large index can take several minutes to convert once; the window remains usable while file search waits for conversion. No full re-embedding is needed.

## Privacy and limitations

Local mode keeps indexed content on your computer; setup downloads need internet access. A remote model server receives the content sent for embedding. Enable LAN/MCP access only for trusted clients. See [security details](docs/SECURITY.md).

AI search can miss or misrank results. OCR can misread text, and sampled video/audio matches give approximate moments. Check original files before relying on a result. VectorDash was developed with AI assistance.

[File support](docs/RETRIEVAL.md) · [MCP setup](docs/MCP.md) · [Build from source](packaging/README.md)

VectorDash is an independent Windows project inspired by [DigUp](https://github.com/ARahim3/DigUp), created by Abdur Rahim. See [acknowledgements](ACKNOWLEDGEMENTS.md). Licensed under [MIT](LICENSE).
