# Clipboard manager

Enable history in the dedicated **Clipboard** settings page. Capture remains off by default; existing application exclusions and Windows privacy markers are respected. Text and images can be enabled separately.

Save an item from its context menu (or Ctrl+P by default) to retain it permanently. Saved items survive retention expiry, history capacity cleanup, restarts and **Clear unsaved history**. The separate saved-storage limit rejects additional saves rather than deleting saved content. **Clear clipboard history** is the explicit confirmation-protected action that also removes saved items.

Text and image retention can differ. Zero hours means no age-based expiry; ordinary history can still be evicted by its item or storage limits. Zero for those limits means unlimited. Values control disk content, not reserved RAM. Text limits are configurable up to 1 MiB; images up to 4 MiB/16 megapixels. Large histories are streamed through SQLite and results remain limited to 100 at a time; image previews load on demand. Exact duplicate copies reuse the existing entry.

Use All, Pinned items, Saved, Text, Images and Links filters. Ctrl+Tab cycles these filters. Saving an item keeps it forever without changing its chronological position. Pinning an item from its context menu also saves it and keeps it at the top of All; unpinning keeps it saved. Images and Saved filters retain semantic image matching when image embeddings are enabled. Names and tags are encrypted and searchable. Context actions allow renaming/editing, saving an image to a file, and pasting into the previous application. Ctrl-click selects multiple items for text copying or deletion. Enter can be configured to paste instead of copy; text currently pastes as plain text. If the previous application cannot be focused, the item remains copied and an error is shown.

OCR and image embeddings are optional and follow idle conditions. OCR uses the configured Tesseract tool; remote embedding servers receive copied images when image embeddings are enabled. Pause capture temporarily or skip the next copy without deleting history.

Export/import saved items as line-delimited JSON. Exported backups contain readable content and can be moved between Windows accounts. Imported records use Windows account encryption again; invalid or over-capacity imports roll back. Automatic capture never records content that predates enabling it.

This first expansion does not capture rich HTML/RTF or copied-file formats, synchronize devices, or automatically expand snippets. Saved, named text entries provide reusable snippets without background typing hooks.

# Shortcuts

All configurable bindings are at the bottom of **Settings → Keyboard shortcuts**. Optional global actions open Search, Clipboard, AI, Web or Emoji directly, invoke dictation, select a screen area into AI, pause capture, ignore the next copy, or stop voice playback. New actions are unassigned by default. Screen-area capture hides VectorDash before selection, then opens AI with the image attached and waits for Send. Canceling restores the previous visibility. In AI, Ctrl+V also attaches an image from the Windows clipboard. Each conversation turn retains its own image thumbnail; click it to view or copy the original.

Windows modifiers are retained, including combinations Windows consumes before ordinary text input. If Windows or another application holds a global combination such as Win+Space, choose Override and Save to intercept only that configured combination while VectorDash is running. Other keystrokes pass through. Protected system shortcuts cannot always be overridden. The Override dialog also reassigns conflicting VectorDash actions; it does not terminate other applications. Reset restores the original Search/Dictate/cycle defaults and clears optional bindings. Save applies changes; Undo restores the last saved settings.

# Mapped network drives

When elevated, file opening is handed to the desktop Explorer session so its drive mappings and credentials are used. This fixes the case where a mapped drive is visible in Explorer but unavailable to VectorDash’s administrator session. This does not create or change drive mappings or networking policies. Folder monitoring still needs the configured source to be accessible to the indexing process; an inaccessible elevated-session drive should be added by its UNC share path.
