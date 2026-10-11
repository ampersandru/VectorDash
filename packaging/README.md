# Building the Windows installer

Run `./Build-Windows.ps1` in PowerShell on Windows with Python 3.14 installed. The script creates a build environment, bundles the application and its dependencies, verifies and sets up a portable Inno Setup compiler, and creates `dist/installer/VectorDash-Setup-0.2.3-x64.exe` plus its SHA-256 checksum.

For an existing Python environment, pass `-Python PATH`. Use `-SkipDependencies` only when its dependencies are already installed.

The installer targets Windows 11 x64, installs for the current user without administrator access, and includes Python, Qt and the media decoder. AI models and optional OCR tools are downloaded separately from Models & hardware. Uninstalling preserves downloaded models, settings and the search index.

For a packaged smoke test, run `dist/VectorDash/VectorDash.exe --smoke-test --smoke-report PATH/report.json`. This checks the bundled UI, image/PDF previews, indexing, search, startup command and media decoder without downloading models.

Build outputs are ignored by Git. Publish the installer and checksum as release assets.
