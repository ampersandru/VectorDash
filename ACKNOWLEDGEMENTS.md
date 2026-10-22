# Acknowledgements

VectorDash is inspired by [DigUp](https://github.com/ARahim3/DigUp), created by
Abdur Rahim. DigUp established the local semantic file-search approach that
inspired this Windows application. Thank you for publishing the project under
an open-source license.

VectorDash's Windows application is written in Python and Qt. It does not compile
or import DigUp's macOS Swift application. The original MIT copyright notice is
preserved in LICENSE. The earlier repository and development history are retained
in the [archived fork](https://github.com/ampersandru/VectorDash-legacy-fork).

VectorDash also uses EmbeddingGemma 2, llama.cpp, Qt/PySide6, NumPy, Pillow,
PyMuPDF, Tesseract, FFmpeg, imageio-ffmpeg, SQLCipher/sqlcipher3 and APSW. These projects, model downloads and
runtime distributions retain their own licenses and notices.

Navigation and window icons use Material Design Icons by Pictogrammers/Templarian, licensed under the Apache License 2.0. The license is included in `windows/assets/mdi/LICENSE.txt`. Source: https://github.com/Templarian/MaterialDesign.

Color emoji artwork: [Twemoji 17.0.3](https://github.com/jdecked/twemoji), by Twitter, Inc., jdecked and contributors, licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Intrinsic SVG dimensions are adjusted for inline display.


## Optional local speech

The optional local TTS download uses [Kokoro ONNX](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX), the [Paradee v2 PDN bundle](https://cdn.mesmer.tools/paradee/paradee-web-v2.zip), [Node.js](https://nodejs.org/), [ONNX Runtime](https://github.com/microsoft/onnxruntime), and the phonemizer package. Paradee's supplied engine, Misaki-derived frontend and lexicon notices are retained in its LICENSES.md. Phonemizer contains eSpeak NG, GPL-3.0-or-later; [upstream source](https://github.com/espeak-ng/espeak-ng). Node and npm dependency licenses remain in the optional runtime. Windows file-URL loader fixes and local import wiring are applied by VectorDash. See [local speech details](docs/LOCAL_TTS.md).
