# Web search

Web search is optional and disabled by default. Enable it in **Web** settings, add at least one provider, and save. Web appears after AI in both search windows and in the Tab/Shift+Tab cycle. Turning it off hides the mode. In **Settings → Search and dictation → Search mode order**, use the arrows to reorder Search, Clipboard, AI and Web. Save to apply the order to both windows and Tab/Shift+Tab; disabled modes retain their place for when you re-enable them.

- **SearXNG:** enter your instance URL. Enable JSON output in the instance configuration (`search.formats` must include `json`). An optional API key is sent as `Authorization: Bearer` for protected instances.
- **Degoog:** enter your instance URL and its API key if required. VectorDash uses the native `/api/search` API, so SearXNG compatibility mode is unnecessary.
- **Brave:** enter a Brave Search API key with Web Search access. Requests use Brave’s official Web Search endpoint.

Select a configured provider inside the query bar. Typing does not send queries: press **Enter** or **Search** to submit. **Stop**, switching modes, or closing the window cancels the active request. An olive trail indicates an active search. The quick popup stays open when clicked outside during a request; it can dismiss normally after completion. Click a result, or select it with arrow keys and press Enter, to open the default browser. Opening a result closes the quick popup. Ctrl+C copies the selected result URL when the query field is not selected.

Requests run in the background, time out after 30 seconds, and accept only HTTP(S) instance/result URLs. Provider redirects are rejected to avoid forwarding credentials; configure the final instance URL. API keys use the existing Windows account encryption. Results are limited to 40 text cards, with no favicon or thumbnail downloads. Web results do not enter the file index and do not use EmbeddingGemma.

API references: [SearXNG](https://docs.searxng.org/dev/search_api.html), [Degoog](https://degoog-org.github.io/docs/api.html), [Brave](https://api-dashboard.search.brave.com/documentation/services/web-search).
