# Security changes and limits

This is an implementation review and a set of regression checks, not an independent audit or proof that every parser is safe.

## Index and credentials

The search index is a disk-backed SQLite database. Encryption is enabled by default: SQLCipher encrypts database pages and the WAL, and the random database key is protected by your Windows account using DPAPI. Names, paths, extracted text, OCR, FTS content and embeddings are covered together. Connections use bounded page caches with memory mapping disabled; temporary scan/replacement databases are encrypted separately. Small SQLite sort buffers, current search results and query vectors still use memory, but the entire database and embedding matrix are never loaded into the process.

You can disable **Encrypt search database** in Settings → Index privacy. The change applies after restarting; a background conversion preserves indexed content and search indexes. Encryption adds disk/CPU work and can affect indexing/search speed, especially on slower hardware. With encryption disabled, filenames, excerpts, OCR and embeddings are readable to anyone who can access the database. Resume checkpoints and saved authentication tokens remain account-protected. A file deleted during conversion may remain recoverable from filesystem backups or unallocated space; enabling encryption is not secure erasure of previously unencrypted data.

Older DPAPI snapshot indexes are converted in the background using bounded chunk reads without a plaintext temporary database or a whole-index memory copy. The original is retained until the replacement is validated, then removed. An interrupted conversion leaves the original recoverable. Potential credentials from older indexes are removed before normal search/MCP exposure when the default sensitive-content filter is enabled.

Remote-model keys and MCP bearer tokens are also DPAPI protected in settings. This prevents a copied database or settings file alone from revealing those values to a different Windows account. It does not protect against a program already running as your account, process-memory inspection, original source files, filesystem backups made before migration, or readable diagnostic logs. DPAPI and original file permissions are separate from whole-drive encryption. See [Microsoft's DPAPI documentation](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata).

Filename and hidden-file checks apply to regular documents and code. Reparse points, symlinks and junctions are excluded. Custom folder and extension exclusions apply at indexing and search time. A default-on content check detects common private keys, cloud/API tokens, JWTs and credential assignments in extracted text, code and OCR before embedding. Suspect content is discarded and the rest of indexing continues. Detection is best effort: an unreadable credential in a picture, unusual formats, or unavailable OCR can evade it. You can disable the content check explicitly. Reindex after changing exclusions to remove old records physically.

## Parser workers

Complex documents, images, code and media are read in a separate worker with Windows low integrity, disabled token privileges, a 1 GiB per-process memory limit, a four-process job limit, bounded output and a 15-minute per-file wall-time limit. Plain text is size bounded. Workers receive no remote-server key or MCP token. Kill-on-job-close also stops decoder/OCR helpers if their worker crashes or is terminated. Cancellation and application exit stop owned workers and their helpers. Office ZIP/XML readers enforce entry and decompression bounds and reject XML entities; archive manifests never extract entries or execute code.

Low integrity restricts writes to ordinary user files. This is not an AppContainer sandbox: network access and read access are not comprehensively blocked, and interactive image/PDF/media previews still use native libraries in the UI process. These remain areas for a stronger brokered sandbox and an external security audit. See [Windows job objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).

## Agent access

MCP is disabled by default and binds to `127.0.0.1` unless LAN access is explicitly enabled. Authentication defaults on. Tokens must be random-looking URL-safe strings of at least 24 characters; generated tokens have 256 bits of randomness. HTTP validates Host and Origin, compares tokens in constant time, limits request size and concurrent connections, and exposes no wildcard CORS policy.

Tools can search, return bounded indexed excerpts for an indexed file, or report indexing status. They cannot read arbitrary paths, write files, start commands, install software or open a file. Returned content is labeled untrusted, and the server instructs agents not to execute document instructions. Users still need to choose trusted agents; authentication does not make a connected agent's behavior safe. Disabling bearer authentication deliberately allows local programs, and local subnet clients when LAN access is enabled, to query the server. The server stops on Exit.

The endpoint implements stateless Streamable HTTP compatibility for MCP revisions 2025-03-26, 2025-06-18 and 2025-11-25. It returns JSON responses and does not offer an SSE event stream. This is local configured bearer authentication, not an OAuth authorization service or a claim of compatibility with every newer draft. See the [transport specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).

## Downloads and distribution

Runtime/model installation requires a publisher-provided SHA-256 checksum and refuses missing or mismatched hashes. Catalog model URLs are pinned to a repository revision. OCR language files use pinned Git object verification; WinGet handles OCR installer package verification. These checks protect integrity relative to the chosen publisher, not against a compromised publisher. The bundled decoder comes from the build dependency.

The setup executable remains unsigned; no authenticated automatic update service is claimed. Release assets include a checksum, but a checksum beside an unsigned installer is not a digital signature. Models and third-party runtimes keep their own notices.

Optional LAN MCP access binds IPv4 interfaces and restricts clients to the active private adapter subnets plus loopback. Host/Origin validation still applies. Bearer authentication remains enabled by default. HTTP traffic is not encrypted; firewall rules and a trusted private network remain necessary. A network adapter change requires applying settings again to refresh the allowed addresses.
