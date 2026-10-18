# Local beta.7: AI Markdown quote layout

A blockquote nested inside a list item inherited the remaining horizontal width after the preceding paragraph. That could push the quote to the far right and leave only a narrow column for text. The locally patched egui_commonmark_backend 0.22.0 ends the wrapping row before measuring a quote frame and again after the quote. Markdown content is unchanged; quote styling, links, emphasis and list structure remain supported.

The rendering regression first failed on the unpatched backend (a quote started at x=683 in a 900-point panel). With the patch, actual painted quote geometry must remain near the left and within the panel at 360, 600 and 900-point widths. Tests also bound quote height to catch one-character columns.

Manual: in main and quick AI windows, request or paste a response containing a linked bullet paragraph followed by an indented > quote. Verify the quote begins below the paragraph, uses the available width, and the next bullet remains below it. Resize the main window and test both a streaming response and a completed response. This setup also contains all previous beta.6 AI Web fixes; it is a local test installer, not a published release.

Verification: 284 native tests passed, 18 environment/opt-in tests skipped. The pre-fix popup also reproduced the quote beside the preceding sentence; corrected main/popup rendering is checked against the same fixture.
