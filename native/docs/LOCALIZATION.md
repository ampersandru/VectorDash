# Interface localization

Settings → Language selects the interface language or follows the Windows UI language. Changes preview immediately in both search windows; use Save/Undo to keep or undo them. No restart or index update is needed.

Languages: English, Spanish, Brazilian Portuguese, French, German, Italian, Russian, Simplified Chinese, Traditional Chinese, Japanese, Korean, Hindi and Vietnamese. Unsupported system languages fall back to English.

## Coverage and boundaries

All 13 catalogs contain the same 709 messages. This expanded pass covers application navigation, settings labels and help, hardware, scheduling, model/server controls, source/index options including RAW, search/clipboard/AI controls, About/update controls, confirmation dialogs, tray menu and app-owned status summaries. A source audit rejects unmarked literal labels in common UI controls. Catalog tests enforce key parity, non-empty values and preserved formatting placeholders.

Filenames, model IDs, microphone/device names, saved-search names, user content, AI answers, GitHub release text, raw diagnostic logs and messages supplied by external servers are retained verbatim. Windows file dialogs use the OS language; the installer currently uses English. These are separate from application setting labels. Translations are machine-assisted and still need native-speaker terminology review; Hindi complex-script shaping also needs native review.

## Implementation

UTF-8 JSON catalogs are embedded in the executable and parsed once. Language changes need no network access or translation service. Stored settings, widget IDs, source signatures, API parameters and AI prompts retain canonical values. Exact messages use `i18n::t`; formatted interface messages use `i18n::tr`, retaining substituted values. Compiled template matchers are initialized once, and a bounded 128-entry per-thread cache avoids repeated work for unchanged statuses. English uses a direct fast path.

Tray menu text reads the active language across threads. Windows system fonts provide CJK/Hindi fallbacks without downloads; machines missing optional language fonts may need the corresponding Windows font feature. Bold text inherits script fallbacks.

## Maintaining coverage

- Mark display strings only, never persisted identifiers, comparisons, user data or API values.
- Add each message to every catalog and preserve all `{...}` placeholders exactly.
- Run catalog/source audits and render both window sizes, including long labels and non-English filenames.
- Keep diagnostic logs and third-party content unchanged so support reports retain their original details.
