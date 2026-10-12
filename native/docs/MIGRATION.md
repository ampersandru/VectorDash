# VectorDash native Rust migration

This branch replaces the Python application runtime with Rust and a native egui/OpenGL interface. It does not call Python, PySide, NumPy or Python parser modules. SQLite/SQLCipher, PDFium and optional llama.cpp, FFmpeg and Tesseract remain native third-party libraries/tools.

The existing full release remains available while this migration is reviewed. Native preview builds install side by side under **VectorDashRust**, use `%LOCALAPPDATA%\VectorDash-Rust`, and can import a **copy** of the existing VectorDash index. The portable ZIP uses a `data` directory beside the EXE; `portable.flag` selects this automatically. Encrypted portable data remains bound to the Windows account/machine through DPAPI.

## Reports being investigated

The numbered performance reports are comments from other users, not reproduced observations from the requester. Separately, the requester's available log showed a 476-second source scan with disconnected network-path errors. No log established the cause of another user's five-second gaps between embedding requests.

The previous text pipeline sends a separate request for each approximately 1,800-character passage. A 2 MiB text file can therefore require over a thousand requests. The native path keeps passages separate for page/moment retrieval while using bounded request batches, a persistent HTTP agent, and adaptive splitting on physical-batch/context errors. There is no intentional delay between normal requests. Counting can be disabled to start processing before complete enumeration. Enumeration and indexing run outside the UI thread; unreadable files are skipped, and disconnected network sources stop after connection failure instead of repeatedly timing out on every child directory.

Rust is not by itself a speed guarantee. Benchmark results must distinguish traversal, model inference, parsing, database work and drawing results. Everything's NTFS journal index and Listary's shell/dialog integration are different workloads; native language choice alone does not establish equivalence.

## Native architecture

- Native egui/OpenGL application and virtualized result rows.
- Disk-backed SQLite/FTS5 with default SQLCipher encryption and Windows-account-protected keys/tokens.
- Background indexing/search channels, bounded parser output, per-file commits, and unchanged-file reuse.
- Owned local model processes have kill-on-close jobs so an abrupt app exit also terminates them. Rust parser workers have kill-on-close Windows job objects, memory/process/time limits; third-party native decoders remain separate tools.
- Lazy local model process: names, applications, settings and calculator searches do not start it. Full indexing and semantic queries load it on demand; the matching encoder is loaded only when media needs it. Remote servers remain under their host's control.
- Native Start menu shortcuts, Windows settings shortcuts and local calculator.
- Standard Windows file-dialog path insertion and Explorer quick-search activation. Custom/elevated dialogs require separate compatibility testing; Copy path remains available.
- Read-only Streamable HTTP MCP endpoint with optional bearer authentication and explicit LAN access.

## Source controls

Choose Quick or Full **before** adding folders, drives or files. A more specific nested Full source overrides its Quick parent. Sources contains manual file/folder exclusions, excluded types, and common Windows/cache exclusions. File counting is optional; with it disabled, progress shows processed/discovered counts rather than an invented percentage of an unknown total.

Downloaded models/runtimes can be uninstalled explicitly. Only managed download directories are eligible, and external model paths/source files are retained.

## Formats

WebP, common raster images and JPEG XL are decoded natively in Rust; AVIF uses the bundled native FFmpeg decoder. PDF text and page rendering use native libraries. Office ZIP/XML documents, text/code, archive manifests and sampled audio/video have separate parsing paths. Unsupported or damaged files remain searchable by name. Actual decoding fixtures and long-file tests are required before declaring coverage complete.

## Building

Install the Rust MSVC toolchain and Visual Studio C++ Build Tools. Run `./Build-Rust.ps1`. The script verifies portable build-tool/PDFium downloads, builds the Rust EXE, packages native decoder tools and third-party notices, and creates an unsigned setup EXE and portable ZIP. No Python runtime ships in either package.

The setup is unsigned; Windows can show SmartScreen or an unknown-publisher warning. AI retrieval can miss or misrank matches. This project was developed with AI assistance.

[OceanScribe](https://github.com/Danmoreng/oceanscribe) was reviewed for its native controller/separate-worker architecture; no implementation code was copied. Original DigUp acknowledgement remains in the repository.

## Measured results and remaining compatibility work

See [benchmark results](BENCHMARKS.md) for every performance profile with encryption on/off, indexing tests, and explicit preview limitations. A native rewrite does not imply feature or performance parity without validation.
