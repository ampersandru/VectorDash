# Search warm-up and model picker test build

This is an unpublished follow-up to Beta 6 on PR 16; no new GitHub beta is created.

- Models and AI → Embedding Model → Remote: enable Warm embedding model when search opens and Save. Hybrid and Semantic search opening sends a tiny real embedding request before typing; File/Folder Names does not. Remote warm-up is off by default. Local model startup retains the existing resource/game checks.
- Chat's model label is now a dropdown. Discovered models are cached per server URL; selecting a model applies to subsequent turns in the current conversation without saving unrelated settings. Settings still defines the default model.
- The updater explicitly requests relaunch after silent installation. An isolated Inno installer test verified both opt-in relaunch and no-launch behavior without touching the installed application.
- Visual image searches no longer gain exact OCR word bonuses or prefilled OCR keyword hits. Explicit screenshot/text queries retain text matching. Singular/plural photo/picture/image prefixes are consistently ignored by keyword tokenization; raw model embeddings are unchanged.

Rank Calibration is a search-time adjustment between text/image/audio channels. It does not classify photographs versus infographics and needs no index rebuild. Compare the exact same query with calibration off/on in All files, Semantic or Hybrid, using mixed Full-indexed content. Try: dogs playing on grass; a dog barking; dog vaccination instructions; someone repairing a bicycle; a car engine running. Record the first five results, their media types and scores. Photo-only queries may show little change because their candidates share one channel. Compare photo of dog / photos of dogs separately to test OCR promotion rather than calibration.
