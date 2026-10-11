# VectorDash

**Find files on Windows 11 by describing what you remember.**

VectorDash uses Google's **EmbeddingGemma 2** to search file contents by meaning, alongside exact-word search. Find a photo of a dog, a topic in a podcast, a clause in a PDF, or an error in a screenshot—all on your computer.

<img width="1070" height="574" alt="VectorDash search interface" src="https://github.com/user-attachments/assets/15456819-3790-4e44-83ff-bd69062a227a" />

## Features

- **Photos and screenshots:** image matching plus optional OCR for exact words.
- **Documents:** PDF pages, Word, Excel and PowerPoint content, with PDF word highlights.
- **Audio and video:** find a topic or scene and open near the matching moment.
- **Multilingual and code search:** search across 100+ languages; optionally index source code.
- **Program launcher:** optionally scan the Windows Start menu, including Windows apps. Program matches appear immediately; select **Programs** or type `app: calculator` to launch without loading a model.
- **Quick search:** a customizable global shortcut, thumbnails, previews and match explanations.
- **Control your sources:** choose drives, folders or individual files. Use fast names-only indexing or full content indexing per source. Filter file types, exclude files, and schedule incremental updates.
- **Choose your hardware:** dedicated GPU, integrated GPU or CPU/system memory. Optional remote llama-server and read-only [MCP access for agents](docs/MCP.md).

## Install

1. Download the latest **Windows x64 setup EXE** from [Releases](https://github.com/ampersandru/VectorDash/releases) (about **91 MB**). Python is included.
2. Run the installer. **The EXE is not code-signed**, so Windows may show a SmartScreen or unknown-publisher warning. If you trust the release, choose **More info → Run anyway**, when available, to continue.
3. For semantic search, open **Models & hardware → Status**, select features, hardware and model, then click **Download, install and start**. **Q8_0** is the default. For names-only search, skip model setup and select Quick mode in Sources.
4. Add folders or drives in **Sources**, then click **Update index**. Start with a small folder to check results.

To enable program launching, open **Sources → Start menu programs** and turn on **Include Start menu programs in search**. The catalog refreshes at startup; use **Scan Start menu** after installing a program. Select a result and press **Enter** to launch its original shortcut, preserving arguments and working folders.

**Names** search needs no model. In **Sources**, choose **Quick file / folder names** or **Full index** before adding drives, folders or files. To change existing sources, select entries (or **Select all**) and apply a mode, then update the index. In either search window, choose **Names**, **Semantic** or **Both**, and use the source dropdown to search some or all sources. More specific folders take priority when sources overlap. Index status shows Full/Quick file counts and how many files have vectors.

Semantic search needs the model and a compatible llama-server. Image/audio search also needs its matching encoder. You can select existing files instead of downloading them. Changing the model or encoder requires rebuilding the index once.

| Shortcut | Action |
| --- | --- |
| **Ctrl+Shift+Space** | Open quick search; change this in Settings |
| **Space** | Open/close the larger preview |
| **Enter** | Open the selected file |
| **Ctrl+C** | Copy the selected file for pasting |
| **Tab / Shift+Tab** | Cycle file categories |

## Models and tools

Approximate sizes vary by release. Model sizes are individual files; runtime sizes are compressed downloads. Installed files, the index and RAM/VRAM use require additional space.

| [EmbeddingGemma 2](https://huggingface.co/ggml-org/embeddinggemma-2-GGUF) choice | Text model | Matching image/audio encoder (mmproj) |
| --- | --- | --- |
| **Q8_0** — default | 310 MB | 555 MB |
| **BF16** | 558 MB | 982 MB |

| Tool | What it does | Approx. size |
| --- | --- | --- |
| [llama.cpp / llama-server](https://github.com/ggml-org/llama.cpp) — CPU | Runs the model using system memory | 20 MB |
| llama-server — Vulkan | GPU/iGPU inference with compatible drivers | 32 MB |
| llama-server — CUDA | NVIDIA GPU inference, including companion runtime | 577–657 MB |
| [Tesseract OCR](https://github.com/UB-Mannheim/tesseract/wiki) — optional | Reads exact words in screenshots and scanned PDFs | 50 MB |
| [OCR language packs](https://github.com/tesseract-ocr/tessdata_fast) — optional | Enables OCR for selected languages | English 4 MB; Bengali 0.9 MB; Arabic 1.4 MB |
| [FFmpeg](https://ffmpeg.org/) — included | Decodes audio/video and extracts frames and audio | 88 MB binary; no extra download |

Install only the runtime for your hardware. OCR is optional for image meaning searches; it helps with exact text. FFmpeg and the encoder are needed for media processing.

## Performance

Recent measurements on the development PC:

| Operation | Measured time |
| --- | --- |
| Search processing with model loaded and index cached | Typically 35–250 ms |
| Scan 3,503 files for changes | About 2.9 seconds |
| Open an 800 MB encrypted index | About 5.8 seconds |

Search timings exclude displaying results. First searches, model loading, hardware and background indexing can take longer. Subsequent indexing processes changed files rather than embedding every unchanged file again.

## Privacy and limitations

Local mode keeps indexed content on your computer; setup downloads need internet access. A remote model server receives the content sent for embedding. Enable LAN/MCP access only for trusted clients. See [security details](docs/SECURITY.md).

AI search can miss or misrank results. OCR can misread text, and sampled video/audio matches give approximate moments. Check original files before relying on a result. VectorDash was developed with AI assistance.

[File support](docs/RETRIEVAL.md) · [MCP setup](docs/MCP.md) · [Build from source](packaging/README.md)

VectorDash is an independent Windows project inspired by [DigUp](https://github.com/ARahim3/DigUp), created by Abdur Rahim. See [acknowledgements](ACKNOWLEDGEMENTS.md). Licensed under [MIT](LICENSE).
