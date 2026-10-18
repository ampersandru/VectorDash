# VectorDash

**Universal search for Windows: files, clipboard, web search, and AI chat in one place.**

VectorDash brings Windows search, a full clipboard manager, web search, and AI chat into one search bar. Press **Ctrl+Alt+Space** from any app to find a file, reopen something you copied, search the web, or ask your AI a question.

Google's **EmbeddingGemma 2** powers search by meaning across fully indexed documents, photos, audio and video, plus optional semantic search for copied images. Find what you remember, even when you do not remember its name. Filename search, apps, Windows settings and calculations work without an embedding model.

**Windows 11 · Windows 10 support is BETA · Setup EXE or portable ZIP**

<img width="996" height="690" alt="20261014-2243-51 0176601" src="https://github.com/user-attachments/assets/2790bc54-4d5c-451a-8117-2259662a7f8f" />

## Windows search

Find files and folders by name, path, exact words, or meaning. **EmbeddingGemma 2** adds semantic document and media search when you use Full indexing.

| Search for | Try searching |
| --- | --- |
| Files and folders | `invoice`, `appdata\local`, `\RHI\nvngx_dlssnr.dll` |
| PDF/Office contents and document passages | `the clause about pets` |
| Photos and screenshots | `photos of dogs`, `a dog on the beach` |
| Video and audio moments | `videos of dogs`, `someone talking about a refund` |
| Apps and Windows settings | `chrome`, `app: calculator`, `settings: sound` |
| Quick calculations | `24*1.2`, `20% of 150`, `sqrt(81)` |

- **Choose your search:** File/Folder Names, Hybrid, or Semantic; filter by file type and source. Names appear while optional semantic results load.
- **Preview before opening:** inspect document passages and PDF pages, browse image/video grids, resize previews, or jump to a matching media moment.
- **Refine and reuse:** use wildcards and filters such as `ext:pdf modified:today`, add exclusions, and save searches.
- **Keep sources current:** targeted file-change monitoring and optional schedules respect your idle, CPU/GPU and power settings.
- **Optional extras:** OCR for exact image text, code indexing, and camera RAW support (**BETA**).

Content and media examples require Full indexing and compatible model/encoder support. See [supported files](docs/RETRIEVAL.md).

## Clipboard manager

Keep and search copied text, links and images. Enable **EmbeddingGemma 2 image search** to find copied pictures by what they show.

| Find something you copied | Try searching |
| --- | --- |
| Notes, text or reusable snippets | `meeting notes`, a phrase, an item name or tag |
| Links | Select **Links**, then enter part of the URL |
| Pictures by meaning | `dog images`, `photos of dogs` |
| Words inside screenshots | `invoice number` with optional OCR enabled |

- **Control retention:** set separate text/image expiry and storage limits, or save items permanently.
- **Save and pin separately:** Saved keeps an item forever; Pinned items also stay at the top of All.
- **Organize history:** filter All, Pinned items, Saved, Text, Images or Links; rename, tag, edit and select multiple items.
- **Reuse what you copied:** copy or paste into the previous app, save images to files, and inspect timestamps and resizable previews.
- **Manage capture:** enable text/images independently, exclude apps, pause capture, skip the next copy, or export/import saved items.

Image-intent queries such as `dog images` prioritize matching pictures when image embeddings are enabled. [Clipboard guide](docs/CLIPBOARD.md).

## Web search

Search through your own **SearXNG** or **Degoog** instance, or the **Brave Search API**. Web search does not require EmbeddingGemma 2.

**Try:** `weather in Seattle`, `latest graphics driver release notes`, or `Rust documentation`.

- **Choose a provider:** select a configured service in the search bar, then press **Enter** to search.
- **Open and share:** click a result to open your default browser, or copy its URL. Optional provider thumbnails add small previews.
- **Stay in control:** stop requests at any time; the popup stays open while a search is running.
- **Use Web with AI:** choose a separate AI web-search provider in Web settings and allow one or two concurrent searches.

Web search is optional and hidden when disabled. [Provider setup](docs/WEB-SEARCH.md).

## AI chat

Talk to your chosen local or remote **OpenAI-compatible LLM server** from either search window. AI chat uses the LLM you configure separately from the embedding model.

**Try:** `Explain this error`, `What does this screenshot show?`, or `Find recent news about this topic and summarize it` with Web search enabled. Follow up with `Make that shorter` or another question.

- **Conversations:** ask follow-up questions, read formatted Markdown, switch discovered models, and choose Brief, Medium or Full responses.
- **Screenshots and images:** attach a monitor, window or selected screen area, or paste an image; requires a vision-capable model.
- **Optional Web access:** let a tool-capable model search through your configured provider and use linked sources in its answer.
- **Voice input and output:** dictate with local Parakeet or a remote transcription server, and use an OpenAI-compatible TTS server for read aloud.
- **Response controls:** explicitly send, stop a request, copy an answer, and pause or stop playback.
- **Agent access:** the optional **MCP server** lets compatible agents search your index and retrieve file information. [MCP setup](docs/MCP.md).

## Make it yours

Reorder enabled search modes, assign direct shortcuts, reset bindings, and choose your theme, accent, transparency and gradient. An offline full-color emoji picker is available beside the search bar.

Local models can unload when idle; optional game/full-screen rules pause indexing and release models. Interface localization is available in 13 languages (**BETA**). In **About**, choose the Release or Beta update channel.

## More screenshots

<img width="1006" height="714" alt="20261015-0700-29 5512708" src="https://github.com/user-attachments/assets/ef656421-b0b0-4842-85b5-9d5266dac6c6" />

<img width="1004" height="696" alt="20261014-2254-19 0296609" src="https://github.com/user-attachments/assets/5d5a8351-277e-4a29-9087-22e537d5398b" />

<img width="982" height="682" alt="20261015-0425-02 7332021" src="https://github.com/user-attachments/assets/2d7ff9ab-b74e-434f-93d0-688ff0e15d14" />

## Install and setup

1. Download the **Windows x64 setup EXE** or **portable ZIP** from [Releases](https://github.com/ampersandru/VectorDash/releases). The setup is about **106 MB**; optional models download separately. Choose a prerelease to test beta features.
2. Install and open VectorDash. The EXE is **not code-signed**; if Windows shows SmartScreen and you trust the release, choose **More info → Run anyway**, when available.
3. In **Sources**, add drives or folders and click **Update index**. Start with **Quick** for filename search, or a small **Full** folder for content and media search.
4. For Full semantic indexing, choose hardware in **Hardware**, then configure **Models and AI → Embedding Model**: download/setup the local model and matching encoder, or connect a compatible remote embedding server. **Q8_0** is the local default.
5. Enable optional features: configure capture in **Clipboard**, providers in **Web**, and your LLM, dictation and TTS services in **Models and AI**. Save your settings. Clipboard, Web and AI modes appear when enabled and configured.

Enable **Include Start menu programs** in Sources to find installed apps. Start menu shortcuts retain their launch arguments and working folders.

### Quick or Full indexing?

| Indexing mode | What it indexes | Uses EmbeddingGemma? | Recommended use |
| --- | --- | --- | --- |
| **Quick** | File and folder names and paths; no file-content extraction or semantic embeddings. Faster indexing with lower resource use. | **No.** Quick indexing and filename searches work without an embedding model. | General files and folders, entire drives, and anything you mainly want to find by name or location. |
| **Full** | Names and paths, plus supported document contents and image, video and audio content according to your indexing settings. Takes more time and storage. | **Yes, by default.** Uses local EmbeddingGemma 2 or your configured remote embedding model to create semantic embeddings. | Images, videos, documents, audio and other supported files whose contents or meaning you want to search later. |

**Recommended:** use Quick for entire drives, then add the document and media folders you care about as Full sources. A more specific Full folder takes precedence over its parent Quick source. Unsupported content remains searchable by name; exclusions still apply.

**Indexing and search modes are separate:** Hybrid combines filename and content/semantic results; it cannot add content matches to Quick-only files. File/Folder Names needs no model. Semantic search and the semantic part of Hybrid need your local or remote embedding model.

Use **Sources** to set exclusions, hidden-file handling, Quick/Full processing order and media options. Automatic updates can follow idle conditions or a schedule. **Clear entire index** removes indexed names, extracted text and vectors while keeping original files, sources, settings and downloaded models.

Switching Local/Remote preserves the index when both servers use compatible models, dimensions, pooling and preprocessing. Use **Rebuild full content** when changing to an incompatible model or encoder.

### Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| **Ctrl+Alt+Space** | Open quick search from any app |
| **Ctrl+Shift+D** | Open quick search and dictate, or start/finish dictation in the current window |
| **Space** | Open/close the larger file preview |
| **Enter** | Open a selected result, copy a calculator answer or clipboard item, or submit AI/Web input |
| **Ctrl+C** | Copy the selected result or AI response, depending on the active mode |
| **Tab / Shift+Tab** | Cycle enabled search modes forward/backward |
| **Ctrl+Tab** | Cycle file types in Search or categories in Clipboard |
| **↑ / ↓** | Select results; page through AI responses |
| **Ctrl+F** | Focus the search bar and select the current query |
| **Double Ctrl** | Open quick search from Explorer or a standard Open/Save dialog when enabled; Explorer can scope it to the current folder |

Customize bindings in **Settings → Keyboard shortcuts** at the bottom of the page. Add direct shortcuts for individual modes or AI area capture, override conflicting bindings where Windows allows it, or reset defaults. Reorder mode cycling under **Search and dictation**. Emoji stays available through its button.

Windows 10 support is **BETA**, targeting x64 version 2004/build 19041 or newer. Windows 11 remains the primary tested platform.

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

Choose **Low memory**, **Balanced** (default), or **Fast search** under **Settings → Search performance**. The filename database stays on disk; unchanged content is reused, and preview/vector caches are bounded. Optional FP16 semantic caching and Rank Calibration are **BETA**. [Benchmarks and limitations](native/docs/BENCHMARKS.md).

## Privacy and limitations

Local indexing and local models keep indexed content on your PC. Remote embedding servers receive content sent for embedding, including copied images if enabled; LLM, TTS and Web services receive their corresponding requests. Enable optional features and MCP/LAN access according to your needs. [Security details](docs/SECURITY.md).

Database encryption is enabled by default. Changing encryption creates a converted copy without re-embedding and retains the original; portable encrypted data is tied to your Windows account/machine. Clipboard exports contain readable backup content.

Semantic results can miss or misrank content. OCR can misread text, sampled media matches give approximate moments, and AI answers need checking. Windows 10, camera RAW, localization and other features marked **BETA** need broader testing. VectorDash was developed with AI assistance.

[File support](docs/RETRIEVAL.md) · [Clipboard guide](docs/CLIPBOARD.md) · [Web search](docs/WEB-SEARCH.md) · [MCP setup](docs/MCP.md) · [Build from source and migration](native/docs/MIGRATION.md)

VectorDash is an independent Windows project inspired by [DigUp](https://github.com/ARahim3/DigUp), created by Abdur Rahim. See [acknowledgements](ACKNOWLEDGEMENTS.md). Licensed under [MIT](LICENSE).
