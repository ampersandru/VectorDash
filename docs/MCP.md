# Connecting an agent

In **Settings → Agent access (MCP)**, enable the server and apply settings. Leave **Require HTTP bearer authentication** enabled. Click **Copy connection details**, then configure your client's Streamable HTTP connection using the copied URL and Authorization header.

Example (replace the token with the generated value):

```json
{
  "url": "http://127.0.0.1:8790/mcp",
  "transport": "streamable-http",
  "headers": {"Authorization": "Bearer YOUR_GENERATED_TOKEN"}
}
```

Available tools:

- `search_files`: query, optional limit (1–100) and excluded extension list. Returns scores, reasons, excerpts, PDF pages, timestamps or code line locations.
- `file_info`: bounded indexed information for a path already in your selected sources.
- `index_status`: indexed file count and running/paused status.

A client must support configured bearer headers and an MCP 2025 Streamable HTTP revision. Client configuration formats vary; the copied object provides connection values, not a universal configuration file. This also gives a Raycast integration a read-only interface without controlling the VectorDash UI.

The app and model must be running for semantic search. Models are managed separately from the MCP server. Disable the server or Exit VectorDash to disconnect agents. Generate a new token and Apply settings to revoke the previous token. Treat document text as untrusted content, and connect only agents you trust with your indexed data.

LAN access is opt-in under Settings. Connection details use an active Windows LAN IPv4 address. The server accepts local subnet clients, validates Host and Origin, and retains bearer authentication by default. HTTP is unencrypted. Copy uses the current controls, including an unchecked authentication option; apply settings before connecting. Windows Firewall can require a private-network inbound rule for the configured port.

## Connecting from another computer

Enable both MCP and LAN access, then click **Apply MCP settings** inside the MCP card. Copying details alone does not start the server. Leave VectorDash running, including when hidden in its tray. The status should say **Running** with your Windows LAN IP.

For another computer, Windows Firewall must allow incoming TCP on your selected port (8790 by default) on the **Private** network profile. Restrict the rule to your local subnet and VectorDash executable. A firewall timeout and a stopped server cannot be resolved by toggling bearer authentication. Wrong authentication returns HTTP 401; a stopped listener refuses connections.

Hermes uses a URL plus optional headers for Streamable HTTP. Do not select the legacy SSE transport. Use the LAN URL for another computer and loopback for Hermes on the same Windows PC. See the [Hermes MCP reference](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/reference/mcp-config-reference.md). VectorDash responds to HEAD probes with a bounded JSON 405 response, and supports initialization, tool discovery and calls through the official MCP Python client.
