# VectorDash

**Find files on Windows 11 by describing what you remember.**

VectorDash searches the contents of your files, rather than relying only on their
names. Search for a dog in a photo, a topic in a podcast, a clause in a PDF, or an
error shown in a screenshot. Exact words, names and invoice numbers work too.

VectorDash is an independent Windows project inspired by
[DigUp](https://github.com/ARahim3/DigUp), created by Abdur Rahim. The Windows app
is written in Python and Qt. See [ACKNOWLEDGEMENTS.md](ACKNOWLEDGEMENTS.md) for credit.

<img width="1070" height="574" alt="image" src="https://github.com/user-attachments/assets/15456819-3790-4e44-83ff-bd69062a227a" />

<img width="1075" height="578" alt="image" src="https://github.com/user-attachments/assets/f6393181-7fcc-4f45-b6bf-a5ff04519ef3" />


## What you can find

| Search example | What opens |
| --- | --- |
| `images of dogs` | Photos matched by what is in the image |
| `payment declined error` | A screenshot, including text read from the image |
| `the clause about pets in the lease` | The matching PDF page, with matching words highlighted |
| `where they talk about sleep` | The matching part of a podcast or recording |
| `zebra in a video` | A sampled video moment showing the zebra |
| `INV-2026-00891` | Files containing that exact invoice number |
| `code: retry a failed call with backoff` | A function or source passage in an enabled project |

You can also search across languages. For example, an English description can
find a Bengali or Arabic note. The model supports more than 100 languages;
results depend on the language and the content.

## Get started

1. Download **VectorDash-Setup-0.2.12-x64.exe** from [Releases](https://github.com/ampersandru/VectorDash/releases).
2. Run the installer and open VectorDash. Python is included; no separate installation is needed.
3. In **Models & hardware → Status**, choose the features you want. Choose your
   hardware and model quantization below, then click **Download, install and start**.
4. In **Sources**, add a drive or folder and click **Update index**.
5. Search for a file. Start with a small test folder to check the results.

To run from source, install Python 3.12 or later, run **Setup-VectorDash.cmd**,
then **Run-VectorDash.cmd**. See [installer build instructions](packaging/README.md).

The app can download llama-server, EmbeddingGemma 2, its image/audio encoder,
OCR tools and language packs. The media decoder is included with the app's
Python dependencies. Downloads can be resumed, and supplied checksums are verified.
A Windows permission prompt may appear when installing OCR.

**Q8_0** is the default model choice. You can select another available quantization
from the download list. Image and audio search need the matching encoder file.
You can use existing model and tool files instead of downloading them again.

## Match explanations, filters and agents

Every selected result shows its match score and why it was found: picture content,
OCR words, document text, filename or meaning. The score combines search signals;
it is not a confidence percentage. Filter by category or exclude extensions such
as `.zip, .mp4, .log` in either search window. Space opens a larger preview and
closing it returns you to quick search.

In **Settings → Agent access (MCP)**, enable a read-only local server for trusted
agents. Bearer authentication is on by default. See [connection instructions](docs/MCP.md).

The Windows index and saved access tokens are encrypted for your Windows account.
Sensitive-content checks and folder/type exclusions are available in Settings.
See [security safeguards and remaining limits](docs/SECURITY.md).

DOCX, XLSX and PPTX content, ZIP file lists and Safetensors metadata now have
special handling. See [file coverage and measured tests](docs/RETRIEVAL.md).
After upgrading, click **Update index** to apply the new extraction and privacy options.

## Search and preview

Select a result to see its preview on the right. Images, PDF pages, text, code and
media moments are supported in both the full app and the popup search window.
Media previews wait for you to press Play.

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+Space` | Show quick search; change this shortcut in Settings |
| `Down`, then `Up` / `Down` | Move through search results |
| `Space` while a result is focused | Open a larger preview; Space again closes it |
| `Enter` | Open the selected result |
| `Ctrl+C` while a result is focused | Copy the file for pasting into Explorer or apps that accept files |
| `Esc` | Dismiss quick search or close the larger preview |

**Show in Explorer** opens the folder and selects the actual file. **Copy path**
copies its location as text. Typing spaces in the search box still works normally.
If another app owns the shortcut, search and indexing still work; choose another
combination in Settings.

## Choose where and how to search

- Add individual folders or whole drives in **Sources**.
- Use a dedicated GPU, integrated GPU, or CPU and system memory.
- Choose an adapter from the detected device list. GPU support needs a compatible
  llama.cpp build; available adapters depend on your runtime and drivers.
- Enable code search only for project folders you choose. Git-ignored and common
  generated files are excluded. Source previews do not execute the code.
- Use **Scheduling** to update at a selected time, while the computer is idle, or
  when both conditions are met. CPU/GPU activity, user inactivity and AC power
  can be used as conditions. VectorDash must be running; it does not wake your PC.
- Start with Windows and choose whether closing the window hides it to the tray,
  minimizes it, or exits the app.

## Understand Status

The Status panel shows a right-aligned indicator for each component:

- **Green:** loaded or checked and ready to use.
- **Yellow:** installed, but not loaded or checked yet.
- **Red:** missing or unavailable.

Select exactly which components to set up. When they are already installed,
the main button becomes **Start**. It is disabled when the selected components
are ready. **Stop** unloads the local model and its encoder. **Cancel setup**
appears only while setup is running.

OCR adds exact text matching inside screenshots and scanned PDFs. It is separate
from the image encoder, which searches by what the image shows. Select OCR
language packs such as `eng`, `ben` or `ara` for the text you want to read.

## Local or remote models

In local mode, selected file content and queries stay on your computer. Internet
access is used for setup downloads. VectorDash uses EmbeddingGemma 2 through
llama-server for text, image and audio embeddings.

You can also connect to a remote llama-server. Select **Remote**, enter the server
URL, optional API key and the model identifier reported by that server, then save
and connect. The server needs EmbeddingGemma 2 in embedding mode and the matching
encoder for images/audio. Remote mode sends selected file content and queries to
that host. Install and stop remote models on the host itself.

## Indexing, speed and limits

Sources shows the number of indexed files and the current indexing activity.
Pause keeps the current job's progress. Cancel keeps the last completed index.
A red animated magnifying glass means indexing is active; purple means it is idle
or paused. File-specific errors are logged and skipped so other files can finish.

The index is stored in SQLite. Search combines meaning, full-text matches and
exact strings. Decoded vectors are cached in memory, and repeated searches reuse
query embeddings. New semantic searches still take time to run through the model.
An indexing request that is already using the model can also delay a new query.

Video search samples frames and scene changes, so a very brief event can be missed.
Audio search uses overlapping 30-second windows, so the match is an approximate
moment. OCR, multilingual matching and semantic results are not always exact.
Unsupported formats remain searchable by filename. Password-protected or damaged
files may be skipped. PDF and image highlights are preview overlays; originals
are not changed.

If you previously indexed with semantic search off, start the model and run
**Full rebuild** once. Rebuild after changing the model or encoder.

## Logs and troubleshooting

Open **Logs** to filter messages by Verbose, Info, Warning or Error. Open the app
log or model output from there when diagnosing a problem. Settings includes a
button to open the local data folder.

Settings, downloads and the index normally live in `%LOCALAPPDATA%\VectorDash`.
Older DigUp data is migrated automatically. Exit the old app before upgrading.
**Exit VectorDash** stops its model and helper processes. Closing to the tray keeps
background search and schedules available. Verified matching local servers are
also stopped on exit; remote servers continue running.

## Development and license

The app is in `windows/`. Install `windows/requirements.txt`, then run
`python windows/desktop.py`. Run the automated tests with:

```text
python -m unittest discover -s windows -p "test_*.py" -q
```

Native desktop checks are in `windows/qa_*.py`. Some media checks require their
optional local fixtures. `qa_search_benchmark.py` measures database/vector search
with 1,500 synthetic files; it excludes model inference and UI time.

VectorDash is available under the [MIT license](LICENSE). Credit also goes to
[DigUp](https://github.com/ARahim3/DigUp),
[EmbeddingGemma 2](https://huggingface.co/google/embeddinggemma-2),
[llama.cpp](https://github.com/ggml-org/llama.cpp), Qt/PySide6, Tesseract and FFmpeg.
Downloaded tools and models keep their own licenses.

## Preview, startup and network access

Press Space on a result to open its larger preview. Press Space again (or Escape) to close it and return to the quick search or main window. Model status lives in Models & hardware. Enable automatic model startup there to load the selected text model and matching photo/audio/video encoder together; this is enabled by default for new installations.

MCP stays on this computer by default. Enable **Allow access from the local network** in Settings to connect trusted LAN agents. Copy connection details includes your Windows network IP and includes a bearer header only when authentication is checked. Apply settings to activate changes. Windows Firewall may require an inbound rule for your selected port on a private network. LAN HTTP is unencrypted; prefer bearer authentication and trusted networks. Remote model servers and LAN agents receive the content you explicitly configure them to access.

## AI disclaimer

VectorDash was developed with AI assistance. AI search can miss files or rank them incorrectly; verify the original file before relying on a result. Local mode keeps inference and the index on your computer. Remote model connections and optional LAN access share relevant content with the configured server or client. This independent project is not affiliated with Google or the original DigUp authors.

## Search appearance and file exclusions

Quick search uses Windows desktop Acrylic for a frosted backdrop, with a semi-transparent eggplant/violet tint in dark mode and a pale lavender tint in light mode. A faint 1px violet stroke, 16px painted corners and subtle elevation replace the bold purple outline. Windows controls the native blur strength rather than an exact CSS pixel radius. Acrylic requires Windows 11 22H2 or newer; older or unsupported desktop sessions use the translucent painted fallback. Model setup progress appears only while setup is active, and logs follow your selected theme.

Right-click a result in either search window and choose **Exclude file from future searches**. This persists across restarts and applies to MCP search results too. Restore files under **Settings → Excluded search files**. Excluding a result does not delete the file or remove its indexed information; use indexing privacy exclusions and update the index to remove content from the index itself.

### Visual search detail

In **Sources**, choose **Balanced** for faster indexing or **High detail** to keep more detail in photos, screenshots and video frames. High detail uses more memory and can help with small objects and text; it does not guarantee a correct match. Enable **Index PDF page pictures and layouts alongside readable text** to search charts, illustrations and page appearance as well as document wording. Each PDF page keeps its own location.

Save the settings, restart the local model, and update the index to apply either change. Remote servers control their own image token budget. Changing these options rebuilds affected content rather than reusing older embeddings. Search combines exact words with semantic results and compares relevance within each content type before combining the results.

### Index progress and interrupted updates

Scheduled updates reuse unchanged files. Sources counts eligible files first and shows how many need indexing, how many are done, how many remain, and how many are unchanged. Progress measures completed files, so a long video or PDF can take longer than a short document.

Completed file work is saved in protected resume checkpoints. If a run is stopped or reaches its scheduled time limit, the next update resumes compatible completed files instead of embedding them again. The previous searchable index remains available until the replacement finishes. A file interrupted partway through is processed again. Changed files, models or extraction settings invalidate the corresponding saved work; a full rebuild intentionally ignores it. Checkpoints are encrypted for your Windows account and removed after a successful update. Large protected indexes use multiple encrypted blocks; existing indexes remain readable.

### Appearance and logs

Settings lets you choose any app theme color (purple by default), plus a separate quick-search popup color, transparency percentage and gradient strength. Color, transparency and gradient changes apply and save immediately. Reset the appearance colors to restore their defaults. The main window has integrated minimize, maximize/restore and close controls; drag the top area to move it, double-click to maximize, and resize from its edges. The sidebar menu button collapses navigation to icons, with tooltips for each section. Search stays highlighted in your chosen theme color.

Logs contains **VectorDash logs** and **Model logs** tabs. Select multiple levels together, such as Warning and Error. Local model output refreshes while its tab is visible; remote model logs remain on their server. The views display the latest bounded log history, while the full log files remain available through the open-file buttons.

Performance updates: searches reuse a protected in-memory read database rather than copying the whole index for every query. Parsed segment locations and repeated file exclusion checks are cached. Unsupported files keep filename keyword search without unnecessary embedding or parser processes. The main window now has 16px rounded corners, and the sidebar collapse button sits beside VectorDash.

### Search controls in 0.2.12

Results use compact rows with image thumbnails, filename, file type and folder. Match details and excerpts remain in the side preview. **File / folder names** is off by default; turn it on to include names and folder paths in keyword matching. Content and screenshot text remain searchable. Visual searches prefer picture embeddings over OCR passage vectors when a picture embedding is available. These changes use your existing index.

Search now resolves detailed results from a bounded pool of semantic candidates plus keyword hits. Debug logs separate index-cache, model/check/wait, ranking and result-display times to help diagnose slow searches.

Both search views show keyboard shortcuts at the bottom right. **Tab** cycles file types and **Shift+Tab** cycles backwards while using search. **↑ / ↓** selects a result, **Enter** opens it, **Space** toggles its larger preview, and **Ctrl+C** copies the selected file. **Esc** dismisses quick search. Settings retain normal Tab navigation.

Shortcut keys are bold in the search footer. To change the global shortcut, click its field in Settings and press the combination you want, then Apply settings. Escape cancels recording. The existing global shortcut pauses while that field has focus and resumes when you leave it.

Startup reuses the sensitive-content check recorded by current protected indexes. Older indexes and indexes built with that filtering disabled still receive the upgrade scan when filtering is enabled. Opening an 800MB protected index measured 5.8 seconds after this change, compared with 11.4 seconds before; this measures index initialization, excluding executable loading and model startup. Startup logs include initialization and index-open timings.

Reindexing counts files using cached source paths and directory-entry metadata, without waiting for model queries or idle scheduling gates during discovery. Pause and cancellation still apply; resource rules apply to indexing work. Counts update while scanning. With no file or source changes, the existing protected index and search cache are retained. A read-only count of the same sources measured 2.9 seconds versus 21.7 seconds before, finding 3,503 eligible files and eight changes. These are isolated profiling measurements, excluding embedding and index commit time.

Both log tabs open at the latest events. Preview metadata and match explanations appear beneath the preview, without repeating the filename. Search and settings dropdowns have rounded controls and popup lists.
