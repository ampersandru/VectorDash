# Local speech

In Models and AI → Voice Model choose Local, select a model and device, then Download / repair local speech. Apply settings. Switching Local/Remote keeps remote URL, protected API key, model and voice unchanged. Existing settings remain Remote by default.

- Paradee v2f lite, v2f and v2g use the supplied PDN WebAssembly SIMD engine on CPU, with the af_heart voice. The archive does not supply GPU graphs; these variants do not claim GPU support.
- Kokoro 82M FP16 and Q8 use ONNX Runtime 1.30.0 on CPU or DirectML. DirectML supports Windows GPUs across vendors; some operators may execute on CPU as part of the runtime's graph partitioning. Unsupported hardware/providers produce an error, not a silent switch to another model. Four English voices are supplied: af_heart, af_bella, am_adam, am_michael.

Local speech is an optional managed Node 22 worker using a pipe, without a listening server. No Python, Docker or user-managed server is needed. The native app manages the worker lifecycle. Text is processed offline; the worker rejects network fetches. Setup downloads the pinned Node runtime, npm libraries through a committed integrity lock, the Paradee bundle and selected Kokoro weights/voices. The model files remain on disk after unloading.

If Read Aloud is checked before sending an AI request, the local model preloads while the main model answers. Unchecking Read Aloud cancels speech and releases the preload according to the selected rule. Immediately unloads the process/model after each generated speech part, before audio playback finishes. The idle option retains it for the chosen number of minutes after generation; the timer runs even when the app is hidden. With idle retention selected, stopping speech discards any in-flight generated audio and lets that generation finish before the idle timer starts; playback stops immediately. App exit kills the owned worker job and releases its model/GPU allocations. Remote services retain responsibility for their own model lifetime.

## Sources and pinned artifacts

- [User-supplied Paradee v2 bundle](https://cdn.mesmer.tools/paradee/paradee-web-v2.zip). SHA-256: `22948ff6b0ae59b2edee04940a35489e97236c72609d3f778359e0446a1f6e09`. Archive README, manifest and licenses remain alongside the installed files. VectorDash applies Windows file-URL and local dependency import fixes to two loaders and the phonemizer import.
- [Kokoro ONNX model and voices](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX), pinned revision and individual LFS SHA-256 in `native/assets/local-tts/downloads.json`.
- [ONNX Runtime Node providers](https://onnxruntime.ai/docs/get-started/with-javascript/node.html), pinned runtime version in package-lock.json. Version 1.23.2 failed the FP16 DirectML ConvTranspose test; 1.30.0 passed the same real generation test.
- [Node downloads](https://nodejs.org/dist/), publisher SHASUMS256 pin in downloads.json.
- Phonemizer includes [eSpeak NG](https://github.com/espeak-ng/espeak-ng), GPL-3.0-or-later; the Paradee engine, frontend and lexicon notices are in the downloaded LICENSES.md. Keep dependency notices with redistributed optional runtime files.

## Testing

The ignored `local_tts::tests::live_optional_speech_all_models_and_unload` test requires an explicit disposable `VECTORDASH_TTS_TEST_HOME`. It installs pinned assets, synthesizes actual WAVs with all five CPU choices and both Q8 and FP16 DirectML, checks immediate and idle unloading, checks preload cancellation, and verifies shutdown. It changes no user speech settings. Listen to the produced audio as well as checking nonempty valid WAVs; numerical validity alone is not a pronunciation or quality benchmark.

## Streaming and provider selection

Voice Model uses a Local/Remote dropdown and retains remote connection settings when Local is selected. Enable TTS streaming starts ordinary AI chat speech from completed sentences before the full response finishes; each local sentence is generated as a WAV, rather than claiming sample-level streaming. Remote audio streaming is negotiated where supported, with buffered fallback (including plain-text unsupported-format errors). Turning streaming off waits for the complete AI response and buffers each audio response before playback. Computer/browser task speech waits until completion to avoid speaking tentative action narration as verified results. Cancel, pause and unload rules still apply.

The isolated `native/scripts/tts_streaming_smoke.py` GUI test uses a six-second mock response and silent WAVs. It checks speech requests start before answer completion when enabled, after completion when disabled, sentence order without duplication, and remote streaming-format fallback. This measures dispatch behavior, not real-model synthesis quality or audible latency.

## Resource status

Hardware → Status shows the active local speech model/device, or the configured model when unloaded. RAM is the speech worker's Windows working set (including its runtime); VRAM is its dedicated GPU allocation reported by Windows, with unavailable counters labelled. Model weights are counted separately from the complete local speech directory, which includes all installed variants, voices and shared runtime files. The weight count is included in the speech-files total, not additional storage. Speech directory totals are cached for one minute and invalidated after downloads/settings changes. Remote server memory cannot be measured locally. Runtime status uses a separate short lock, so sampling does not wait for an active synthesis request.

## Linked text

Speech uses visible Markdown text rather than link destinations. Inline links, emphasis and code spans preserve adjacent words and punctuation (for example a linked name followed by `'s`); only block boundaries add whitespace. Reference-link labels remain readable when their definitions arrive later in a streamed response. Unit tests compare linked and plain text, a delayed streaming fixture verifies the exact text sent to speech, and an optional real Paradee fixture generates audio from a linked sentence.

### Additional Paradee voices (October 27 update)

Local Speech settings offer Heart, Wren, Everly, Hadley, Bryony, Maeve, Miles, Trevor, Edwin and Hugh. Heart retains the three Paradee model variants. The other voices each use their own compact CPU model distilled from Gradium Phonon; selecting one downloads only that voice and its shared engine when Install/Update is pressed. Switching back to Kokoro restores the supported Kokoro voice choices. All voice files and engine components are checksum-pinned; licenses ship with the optional runtime.

Voice selection is retained between requests and included in the worker identity, so switching voices reloads the correct model. Existing preload, streaming and unload rules apply.

Credits: [Mesmer.tools](https://mesmer.tools/free-tools/tts-generator), [Gradium Phonon](https://huggingface.co/gradium/phonon-31m-en) and [Phonon voices](https://huggingface.co/gradium/phonon-voices).
