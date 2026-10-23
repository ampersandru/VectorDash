# VectorDash Markdown layout and source-link patch

Based on egui_commonmark 0.22.0 (MIT OR Apache-2.0; licenses retained).

The viewer explicitly wraps text, and table columns share the available width with wrapping cell layouts instead of expanding to their longest unwrapped contents. This prevents a wide table from enlarging the scroll content and pushing later paragraphs beyond the window.

Plain HTTP(S) text outside code, images, and existing links uses the backend source-link renderer. Existing Markdown links, autolinks, references, code, and source strings remain intact. Regression tests check painted geometry and actual link click destinations.
