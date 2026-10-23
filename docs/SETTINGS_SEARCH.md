# Settings search and speech status

Every settings panel has a fixed Search settings field above its scrollable content. Results from the current panel precede matches from other panels. Results search the setting label, translated label, section and aliases; search runs locally without an AI or network request. Click a result (or press Enter for the first result) to open the relevant section. Navigation keeps the query available and does not change preferences. The field is capped at 256 characters.

The offline catalog is generated from settings labels plus aliases with `python native/scripts/generate_settings_catalog.py`. Regenerate it after adding labels, then run unit and UI tests. Section anchors are explicit: results navigate to a section, rather than automatically toggling a disabled feature or switching Local/Remote. Idle conditions remain discoverable while disabled and explain which option enables them.

Hardware status order: llama-server, EmbeddingGemma 2, Dictation, Speech, photos/audio/video encoder, Tesseract OCR, FFmpeg, PDF renderer, Camera RAW decoder. Remote Speech displays its configured model and the same readiness styling as remote Dictation. Retained local downloads remain visible separately from server-managed memory.

The Speech Uninstall button removes all downloaded local speech models and their shared runtime under the managed local-tts directory, after confirmation and stopping speech. Other models, index data and remote settings remain intact. A remote model itself is managed by its server and is not deleted by this button. Navigation and opening the confirmation do not remove files.

Validation: ranking, localization, panel coverage and scoped speech removal have unit tests. `native/scripts/settings_search_smoke.py` uses disposable profiles and localhost metadata to exercise ranking, Voice Model/Clipboard/Scheduling navigation, unchanged preferences, remote status and the uninstall confirmation. Fixture files must remain intact until explicit uninstall confirmation; the removal unit test deletes only its own temporary speech folder.
