# Launcher tools

Search, Emoji, Clipboard and AI share the main and quick-search windows. Filename/semantic search remains independent.

## Emoji and clipboard

Emoji uses a bundled Unicode catalog and CLDR keywords in a scrollable grid. Hover for names; click or Enter copies an emoji.

Enable clipboard history in Settings to capture newly copied text, links and images. Choose retention hours and excluded app executable names. Privacy-marked entries are skipped. The separate disk database holds Windows-account-protected payloads: at most 500 entries / 128 MiB; text 16 KiB, PNG images 4 MiB / 16 megapixels. Only the selected image is decoded. Enter/Ctrl+C restores the selected text/image. File-copy formats are not captured. History is not indexed or exposed through MCP.

## AI

AI settings contains the /v1 URL, protected optional key, model refresh/dropdown, estimated context budget, vision toggle and search shortcut (Tab by default). Tab changes mode; Enter/Ask sends. Follow-ups retain prior turns, bounded by the budget. Stop/Escape cancels; Ctrl+C copies the latest reply; wheel scrolls and Up/Down pages. Markdown does not automatically fetch remote images.

Screenshot selects a monitor or open window. Review the capture, optionally drag an area, then attach. Only Ask/Enter sends it to the configured server as OpenAI image_url parts. A vision model is required. One screenshot per turn, at most 1600 pixels and 2 MiB JPEG. Screenshots are not saved to the file index. Conversation image memory is bounded.

## Dictation

Models & hardware optionally downloads Parakeet TDT 0.6B v2 and native sherpa-onnx. CPU uses int8; NVIDIA CUDA uses fp16 plus CUDA/cuDNN libraries. This is English speech-to-text, not speech synthesis. CPU downloads ~500 MB, CUDA ~3 GB. A compatible NVIDIA driver is required for CUDA. All model/runtime/library downloads are SHA-256 pinned; no Python runtime is installed.

Click the red microphone or configurable Ctrl+Shift+D search shortcut to record up to 60 seconds. Click again to transcribe into the query, without submitting AI. Escape/window close cancels. The inference child loads only for transcription and exits afterward, releasing model memory. Temporary WAV recordings are removed after completion/cancellation. Audio stays local.

Model: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2
Conversion: https://k2-fsa.github.io/sherpa/onnx/pretrained_models/offline-transducer/nemo-transducer-models.html
Raycast research: https://manual.raycast.com/emoji-symbols and https://manual.raycast.com/clipboard-history
