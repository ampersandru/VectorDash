"""Restore writable protected snapshots without SQLite's memdb growth ceiling."""
import sqlite3


def restore_writable(connection,snapshot):
    # deserialize() uses SQLite's memdb VFS, which defaults to a 1 GiB cap.
    # Copy into an ordinary :memory: pager before writing. Both buffers remain
    # in memory; only the encrypted snapshot is ever persisted by the engine.
    source=sqlite3.connect(':memory:')
    try:
        source.deserialize(snapshot)
        source.backup(connection,pages=1024)
    finally:
        source.close()
