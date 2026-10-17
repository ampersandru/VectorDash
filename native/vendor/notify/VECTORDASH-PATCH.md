# Local notify 8.2.0 patch

Source: crates.io notify 8.2.0, CC0-1.0 (LICENSE-CC0 retained).

Only src/windows.rs changes. ReadDirectoryChangesW startup/completion failures now reach the application callback with the originating watched root before the backend removes the watch. Successful zero-byte completions produce a root-scoped Rescan event after rearming, instead of parsing an empty buffer. VectorDash retries the failed watch independently with capped backoff and reconciles only its source on reconnection. Normal notifications and non-Windows implementations remain upstream code.

Remove this patch after equivalent upstream fixes are available. Validate real directory create/delete/rename delivery, root removal/recreation and notification overflow before upgrading.
