# VectorDash

**A local Windows 11 search replacement and launcher powered by EmbeddingGemma 2. Built in Rust.**

## What it does

- **Quick indexing:** find files and folders across whole drives without loading a model.
- **Full indexing:** search the meaning of documents, pictures, screenshots, audio and video; jump to matching pages or moments. Nested Full folders override Quick drives.
- Launch **apps and Windows settings**, or calculate directly in search.
- A global shortcut opens quick search with previews, file/source filters and familiar keyboard actions. Double Ctrl also works in Explorer and standard Open/Save dialogs.
- Optional **automatic change indexing** and scheduled scans respect your idle limits. Pause/cancel indexing, reuse unchanged content and inspect the **File log**.
- Choose CPU, GPU or iGPU, install optional tools, and unload idle models automatically. Names search stays model-free; semantic searches load the model on demand.
- Customize light/dark themes and the translucent popup. One **Save / Undo** bar handles settings changes; **About** includes updates and changelogs.
- Optional read-only **MCP server**, bearer authentication and LAN access. The on-disk index is encrypted by default; performance profiles balance memory and speed.

Measured warm searches on a copied million-file index: about **10 ms names / 30 ms semantic** in Balanced mode with encryption and a warm GPU model. Empty UI measured about **115 MiB**, excluding models and helpers. Startup, cold queries and your files/hardware affect results. [Measurements](native/docs/BENCHMARKS.md).

## Install

Download the **setup EXE or portable ZIP** from [the latest release](https://github.com/ampersandru/VectorDash/releases/latest). **The EXE is unsigned:** Windows may show SmartScreen or an unknown-publisher warning.

Add sources as Quick or Full, then choose **Models & hardware** for semantic features. Enable automatic change indexing in **Scheduling**, and click the red **Save** button. Notifications work on readable NTFS and exFAT folders while VectorDash is running; recovery scans catch missed changes. No persistent USN journal replay is implemented yet.

Existing Rust installations keep their settings/index. Older Python users can import a **copy** of their index in Sources. Portable data is stored beside the EXE; encryption is tied to your Windows account/machine. Conversion retains the original database, including any plaintext copy.

## Optional tools

Approximate download sizes vary by release and quantization.

| Model/tool | Purpose | Approximate size |
|---|---|---|
| EmbeddingGemma 2 GGUF | Semantic query and document embeddings | About 310 MB Q8 / 558 MB BF16 |
| Matching mmproj GGUF | Image/audio/video encoder for the same model | About 555 MB Q8 / 982 MB BF16 |
| llama-server | Runs the model on CPU, CUDA or Vulkan | Hundreds of MB; backend-dependent |
| Tesseract OCR + language data | Exact words inside screenshots/images | About 50–150 MB |
| FFmpeg / FFprobe / FFplay | Media extraction and playback; bundled | About 150 MB unpacked |
| PDFium | PDF extraction/page preview; bundled | About 10–25 MB |

[Supported files](docs/RETRIEVAL.md#file-coverage) · [Security](docs/SECURITY.md) · [Build and migration](native/docs/MIGRATION.md)

AI search may miss or misrank matches. Developed with AI assistance. Independent project inspired by [DigUp](https://github.com/ARahim3/DigUp); [credits](ACKNOWLEDGEMENTS.md). [MIT license](LICENSE).
