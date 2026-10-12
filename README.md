# VectorDash — native Rust preview

**A local Windows 11 search replacement and launcher, powered by EmbeddingGemma 2.**

This PR replaces the application runtime with **Rust** and a native egui/OpenGL interface. The new EXE, setup and portable package do not ship Python, PySide or NumPy. Optional llama-server, OCR and media tools remain native external programs.

The stable Python release remains available while this migration is reviewed. Test builds install beside it, use separate settings/data, and can import a **copy** of its index.

## Highlights

- **Quick names** indexing for drives, folders and files; **Full** indexing for document text, photos/screenshots and sampled audio/video moments. Specific nested Full sources override their Quick parents.
- Search **Names**, **Semantic** or **Both**, with file-type/source filtering, previews, match explanations, exact-word retrieval, app launching, Windows settings shortcuts and a calculator.
- **Lazy models:** names, apps and calculations load no model. Full indexing and semantic queries load EmbeddingGemma 2 on demand; media indexing loads the matching encoder only when needed.
- **Optional counting:** start processing immediately in a single pass, or enable pre-counting for percentage progress. Background indexing has pause/cancel, unchanged-file reuse, skipped-file logging and bounded streaming queues.
- **Sources exclusions:** add files/folders manually or with a picker; exclude types, common cache/system folders or individual results.
- **Explorer/dialog integration:** double Ctrl opens quick search from Explorer or standard Windows Open/Save dialogs; selecting a result can fill the dialog's file-name field. Custom/elevated dialogs may need Copy path.
- **WebP, AVIF and JPEG XL**, native PDF-page previews with text highlighting, Office/text/code extraction and media moments.
- **Setup controls:** choose CPU, GPU or iGPU; optional protected download tokens, verified downloads, and uninstall controls for managed models/runtimes.
- **Read-only MCP** with optional bearer authentication and explicit local-network access. Default encrypted, disk-backed SQLCipher storage.

[Native preview 0.3.3 changes](native/docs/0.3.3.md): idle checks exclude the app and its model/helpers; faster HTML and filename-only processing; clearer index status; scrolling, opening and copying fixes; wildcard exclusions and Explorer folder scoping.

## Try the preview

1. Close the stable VectorDash while testing to free its global shortcut.
2. Run the unsigned native setup, or extract the portable ZIP and run `VectorDash.exe`.
3. Add Quick/Full sources in **Sources**, or import a copy of the existing index. The original index stays unchanged.
4. For semantic indexing, configure or install the runtime/model/encoder in **Models & hardware**. They remain unloaded until needed.

**The installer is unsigned.** Windows may show SmartScreen or an unknown-publisher warning. The portable ZIP stores data beside the EXE; encrypted data is bound to the Windows account/machine. Encryption conversions create a separate copy and preserve the original. Old plaintext copies remain readable until removed.

## Build

Install Rust's Windows MSVC toolchain and Visual Studio C++ Build Tools, then run `./Build-Rust.ps1` or `./Build-Windows.ps1`. Verified build dependencies and native decoder/renderer tools are packaged automatically. `cargo test --manifest-path native/Cargo.toml` runs native regression tests. The previous builder is retained as `Build-Windows-Legacy.ps1`.

Measured warm backend searches on a copied million-file index: approximately **10 ms names / 30 ms semantic** in Balanced mode with encryption enabled and a warm GPU server. Empty GUI peak about **115 MiB**; model/helper memory is additional. See [measurements and limits](native/docs/BENCHMARKS.md).

See [migration details](native/docs/MIGRATION.md) for architecture, testing, compatibility and benchmarks. No claim of Everything/Listary performance parity is made without a matched comparison; standard-dialog support does not imply every custom dialog is supported.

AI search can miss or misrank matches; verify originals. VectorDash was developed with AI assistance. It is an independent project inspired by [DigUp](https://github.com/ARahim3/DigUp), with original credit preserved in [acknowledgements](ACKNOWLEDGEMENTS.md). [MIT license](LICENSE).
