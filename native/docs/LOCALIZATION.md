# Interface localization — first implementation

Settings → Language selects the interface language, or follows the Windows **UI language** (not its regional number/date format). Changes preview immediately in both search windows. Use the existing Save/Undo bar to keep or undo the selection. No restart or index update is required.

Languages: English, Spanish, Brazilian Portuguese, French, German, Italian, Russian, Simplified Chinese, Traditional Chinese, Japanese, Korean, Hindi, Vietnamese. Portuguese regional variants currently use Brazilian Portuguese; Spanish regional variants share one catalog. Unsupported system languages fall back to English.

## Current coverage

Each catalog contains the same 99 core messages: navigation, search placeholders, search modes and file types, search/AI/Clipboard controls, common source/exclusion/index actions, Settings section labels, Save/Undo, playback tooltips, result summaries and footer shortcut actions. The interface language does not change queries, filenames, saved search values, API model IDs, AI prompts/responses, log records or the search index.

This is **partial interface localization**, not a claim that every screen is translated. Advanced settings, detailed tooltips, confirmations, installer/tray text, detailed indexing/model states and errors still use English. Translations need review by native speakers, especially technical vocabulary and Hindi text shaping. Glyph availability alone does not verify complex-script shaping. Right-to-left languages are outside this first set.

## Implementation

UTF-8 JSON catalogs live in `native/locales`. English display text is the stable message key. Missing keys safely display their English key; unsupported locale preferences resolve to English. Catalogs are embedded and parsed once; language selection performs no network requests and adds no service/dependency. Settings keep canonical English enum values and persist a separate language code.

Windows system font fallbacks cover the supported scripts; no fonts are downloaded or bundled. Nirmala collections are supported as well as Segoe UI, Microsoft YaHei and Malgun. Japanese/Traditional Chinese interfaces prefer their installed regional font. Fonts are read only at initial setup or when the effective language changes. Machines missing optional language fonts may need the corresponding Windows language/font feature. Bold text inherits script fallbacks too.

## Extending coverage

1. Add the display-only call through `i18n::t`; never translate IDs, comparisons, protocol values or saved enums.
2. Add the same key to every catalog. Keep product names, API paths, shortcuts and technical identifiers unchanged.
3. For dynamic messages, add a full message template with named placeholders and validate that every translation preserves those placeholders. Do not concatenate translated fragments for grammar-sensitive messages. The current count summaries intentionally use a compact noun label without singular/plural inflection; grammatical plural templates are a follow-up.
4. Test Save/Undo, restart, both window sizes, long German/Portuguese labels, Hindi shaping, CJK glyphs and non-English filenames.

## Follow-up work

- Translate advanced settings, help text and confirmation dialogs in coherent screen-sized groups.
- Introduce parameterized messages/plurals before migrating detailed worker statuses/errors.
- Localize tray/installer text and audit dialog buttons supplied by Windows.
- Native-speaker terminology and layout review for each language; complex-script shaping support as needed.

## Validation of this first pass

- 166 automated tests passed; 10 existing opt-in tests were not run. Includes catalog key parity, locale fallback/region mapping, saved language migration, canonical search settings and unchanged indexing source signatures.
- All 13 languages rendered in isolated application profiles, alternating main/compact popup windows and dark/light themes; German, Japanese, Spanish and Hindi screenshots were inspected.
- Installed Windows fonts covered every character in all 13 catalogs on this Windows 11 test machine. This optional-font coverage check reports missing font features on other machines; it does not establish native-speaker or complex-script shaping correctness.
- The application build and formatting check passed. No release/install or changes to the user's active index were performed for this PR.
