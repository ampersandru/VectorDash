# Compact popup dialog sizing

Unreleased follow-up on PR #16 after 0.3.25-beta.4.

The screenshot chooser is an egui window inside the search viewport, not a separate native window. Previously only Filter/dropdown state expanded that viewport, so the screenshot chooser could be constrained to a 254-pixel host and lose its title/controls.

- Screenshot selection, sent-image viewing and changelog dialogs expand the popup to at least 640 pixels. Save Search, clipboard clearing, delete/uninstall/index confirmations, Filter and dropdown menus request at least 500 pixels. Closing the final dialog/menu restores the normal empty Search/AI height.
- All in-window dialogs use a shared viewport-constrained, scrollable builder. The separate native Quick Look window retains its normal sizing; its embedded fallback also uses the constrained builder.
- Screenshot selection uses bounded-height preview/list columns. Long window titles truncate with full-title tooltips. Attach stays below the preview rather than inside a growing list. Sent-image previews reserve room for Copy.
- Dialog state now also prevents the popup host's focus-loss dismissal from interrupting these dialogs.

Validation: application build and formatting passed; 173 tests passed (10 existing opt-in tests not run), including a geometry regression that resizes a long-content dialog through 940×640, 780×500 and 780×254 hosts. Eighteen harmless UI fixture checks covered all nine dialog types, empty/landscape/portrait screenshot previews, visible Attach/Copy actions, main-window capture, Filter/type dropdowns, and restoring compact heights after closing capture/Save Search/sent-image dialogs. Destructive confirmation dialogs were rendered without activating their actions. No actual screenshots, external servers or active user index were used by these fixtures.
