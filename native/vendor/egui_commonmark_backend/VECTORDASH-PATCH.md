# VectorDash Markdown layout patch

Based on egui_commonmark_backend 0.22.0 from crates.io (MIT OR Apache-2.0; original license texts retained).

The blockquote helper ends the current wrapping row before allocating its frame and ends the quote row afterward. List items use a horizontal wrapping layout; without these boundaries, a quote following a paragraph can receive only the few pixels left at the right edge and push text outside the response panel.

No Markdown source is rewritten. Links, emphasis, list indentation, source text and quote decoration remain supported. The application regression test renders quoted sources inside lists at narrow and wide widths and checks the actual painted text geometry.
