# Building the Windows installer

Run `./Build-Rust.ps1` in PowerShell with the Rust MSVC toolchain and Visual Studio C++ build tools. It stages the native application and PDF runtime, builds the Inno Setup installer, and creates a portable ZIP and SHA-256 manifest under `dist/installer`.

The current installer supports Windows 11 x64 and **Windows 10 version 2004/build 19041 or newer (beta)**. It installs per user and preserves settings, downloaded models and the index on uninstall. Models and optional tools are downloaded separately.

Use `-SkipBuild` only after building the current native source. Validate artifacts with `native/scripts/release_gate.py --verify-only` before publication. Published release assets are authoritative; legacy Python packaging files are retained for migration/reference.
