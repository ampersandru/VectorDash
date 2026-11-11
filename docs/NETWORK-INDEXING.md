# Network indexing

Network drives use Windows directory change notifications as their fast path. Ordinary file notifications queue exact paths; genuine directory creation/rename can require subtree discovery. No NAS helper or new runtime download is required.

## Notification boundaries

The Windows notification buffer is 64 KiB, within the documented SMB limit. A network root with at most 128 top-level entries and at most 32 allowed direct subdirectories can use a nonrecursive root watch plus recursive branch watches. Wider roots retain one recursive watcher, keeping thread/handle growth bounded. New branches are attached automatically; exceeding the branch cap falls back to a recursive root before branch watches are removed. Local NTFS journal behavior is unchanged.

Overflow, watcher reconnection and provider-requested rescan retain the watched branch's identity. A failure in a branch does not automatically escalate to the configured share root.

## Incremental safety and gap recovery

The optional periodic safety pass defaults to five minutes **after the prior pass completes**. A pass enumerates directory metadata, not file content. Normal notifications remain the immediate update path. Startup/offline gaps and known missed-event recovery use required incremental passes even if periodic safety is disabled.

A background worker checks at most 256 directory/missing-child entries per batch, at most 16 small directories, and yields at a nominal 100 ms budget between blocking operations. A single stalled SMB call can last longer; it does not block the UI or monitor scheduler. Only one worker batch is active. Automatic-indexing hold, resource/idle conditions and an active indexing job pause further batches.

On Windows, directory-entry metadata avoids an additional stat call for every file. New/changed files and confirmed missing immediate children are durably queued for the normal indexing pipeline. Directory records can be refreshed without recursively queuing that directory. Complete directory enumeration is required before considering missing children; an unreadable or partial listing retains their index entries. Engine-side existence checks still protect deletion handling.

Directory work, temporary seen-entry sets and file changes live in the existing index database (including its encryption policy). Progress is committed with changed paths. Periodic work can resume safely after an interrupted directory. A new offline/monitoring gap starts a fresh metadata pass for the affected source or branch, because earlier directories may have changed while notifications were unavailable. Temporary seen rows are removed after each directory completes. A parent index supports immediate-child comparisons without querying every indexed file for every directory.

A completed explicit full scan retires older reconciliation requests before timers run; genuinely newer monitoring gaps survive. Disabling periodic checks cancels periodic work without dropping required recovery. Logs report batch and current-run metadata counts separately from normal indexing counts.

## Limits

A complete safety pass still performs O(N) metadata enumeration. The improvement is bounded/background work, fewer metadata round trips and only indexing differences—not a promise of constant-cost reconciliation. SMB notifications cannot replay arbitrary offline changes, and remote NTFS USN access is not available through an ordinary mapped share. Server-side change tracking would require a separate optional service and is not included.

Broad explicit manual updates, genuinely new/moved-in directory trees and local NTFS journal gaps can still require larger scans. An inaccessible branch is retained in the index and reported; periodic passes or subsequent recovery/manual updates can retry it. The first reconciliation creates a database parent-lookup index, which can take additional time on a large existing database without enumerating the filesystem.

## Windows API references

- [ReadDirectoryChangesW network support, overflow behavior and 64 KiB limit](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw)
- [SMB recursive change notifications](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-smb2/598f395a-e7a2-4cc8-afb3-ccb30dd2df7c)
- [USN journal operations are not available over ordinary SMB shares](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_read_usn_journal)
- [Rust directory-entry metadata on Windows](https://doc.rust-lang.org/std/fs/struct.DirEntry.html#method.metadata)
