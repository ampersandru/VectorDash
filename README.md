# VectorDash

**Find files on Windows 11 by describing what you remember.**

VectorDash searches the contents of your files, rather than relying only on their
names. Search for a dog in a photo, a topic in a podcast, a clause in a PDF, or an
error shown in a screenshot. Exact words, names and invoice numbers work too.

VectorDash is an independent Windows project inspired by
[DigUp](https://github.com/ARahim3/DigUp), created by Abdur Rahim. The Windows app
is written in Python and Qt. See [ACKNOWLEDGEMENTS.md](ACKNOWLEDGEMENTS.md) for credit.

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

1. Download **VectorDash-Setup-0.2.0-x64.exe** from [Releases](https://github.com/ampersandru/VectorDash/releases).
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
| `Space` while a result is focused | Open a larger preview |
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
