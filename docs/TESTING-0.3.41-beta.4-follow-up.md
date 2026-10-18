# Search and mapped-folder regression follow-up

Changes added after the published beta.4 build; these are not in that existing release asset.

- Web result selection follows the configured theme color; the in-flight Web border remains olive.
- Clipboard queries with explicit image intent (for example `dog images`, `dog photos`, or `photos of dogs`) normalize the embedding query and rank matching images before literal text; ordinary queries keep literal text priority and explicit pins remain first.
- Double-Ctrl accepts the filesystem path resolved by Explorer without checking it using VectorDash's elevated drive mappings, and falls back to Explorer's file URL for mapped drives and UNC folders.
- File-icon lookup falls back to the registered extension when the path is inaccessible, rather than using a generic application icon; transparent icon pixels retain their alpha.
- Asynchronous search filters confirmed missing fixed-local-drive paths from the bounded result list; access-denied, remote, removable and virtual paths are retained. Search never deletes database records. Activation also checks for a local file disappearing between search and Enter and reports a clear error.

## Validation

279 native tests passed (17 environment tests skipped), including real temporary files for early lexical results, explicit image-intent clipboard ranking, deleted local results, and mapped/UNC URL conversion. The opt-in live shell test separately passed against the user's existing Explorer window at T:\ and verified PDF/video association icons for inaccessible paths. 11 Web GUI scenarios plus optional provider thumbnail loading passed; 15 clipboard/shortcut/attachment GUI scenarios also passed.

## Manual checks

1. Set a visibly different theme color, search Web and confirm the selected row uses it.
2. With clipboard image embeddings enabled and images already indexed, compare `dogs` with `dog images`; test All, Images and Saved. Explicit pins remain at the top of All.
3. Open a known existing local file and a known existing T: file from Search; an absent file cannot be launched by adding its parent as a source.
4. In Explorer on T: or a UNC subfolder, double-Ctrl and confirm the scope is that folder; test nested folders and multiple Explorer tabs.
5. Check PDF, video and image fallback icons for correct file association and transparent edges. Video thumbnails still require that the video is accessible.
6. Delete a disposable indexed local file, then search before an update: the missing hit should be omitted, while existing neighboring files still open immediately.

Diagnostics found that the reported local Videos/1575314184.mp4 and other example video paths were absent from the filesystem. Logs also showed T: watcher access failures in the elevated session. These are distinct from a valid source being configured: sources do not restore moved/deleted files or create drive mappings in another Windows token. No user index, mappings or files were modified during diagnosis.
