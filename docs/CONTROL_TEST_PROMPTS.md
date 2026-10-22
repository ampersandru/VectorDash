# Computer and browser control test prompts

Use Qev **Execute** with vision enabled for the first pass. Repeat selected prompts with **Off** using the same main model to compare total time, action count, Qev calls/executions, fallbacks, and cache reuse. Export diagnostics after each response. A fast final answer is not success unless the requested application state changed.

Start each numbered scenario in a new chat; send follow-ups in that same chat. Do not manually help unless it gets stuck; record where help was needed. Stop repeated actions if there is no progress.

## Spotify: library and follow-up

1. Open Spotify and play my Release Radar playlist. Use its existing library entry if available. Verify the playlist and active playback once, then stop.
2. Now play ZHU's latest single. Use Spotify's Latest Release label to identify it. Once the matching release and active playback are verified, stop; don't reopen the artist page to verify the same facts again.
3. Now play John Summit's latest single. Tell me the track, release, and playback device. Don't switch playback devices unless I ask.

## Spotify: navigation and dropdowns

1. Open Spotify, search for Kaskade, then replace the search with John Summit. Dismiss suggestions, open John Summit's artist page, select Singles and EPs, and play the newest listed release. Verify once and stop.
2. Pause it, then return to my Release Radar playlist and start playing it.

## Spotify: reading and scrolling

Open Spotify and open John Summit's artist page. Scroll the main content pane to Discography, select Singles and EPs, and list the first three releases shown. Do not play anything or alter my library. If scrolling doesn't move the page, change strategy instead of repeating the same input.

## Notepad: text and follow-up

1. Open a NEW blank Notepad tab, preserving all existing tabs. Type this two-line text exactly, then replace every dog with cat and every cat with flying cow:
The dog found another dog.
The dog says hello!
2. Append a blank line and a new joke. Preserve the existing two lines and verify the appended text appears only once.
3. Save this new document as vectordash-control-test-01.txt on my actual Desktop using the observed Desktop location. If the file exists, stop rather than overwrite it. Verify the filename and unmodified state.

## Restaurant research: browser control

Test assumptions: two people; $50 per person for food before tax/tip. Change these if needed. October 30, 2026 is Friday. Use the exact date so relative dates do not drift.

Use browser control to research Italian restaurants near Bernal Heights, San Francisco, for two people on Friday, October 30, 2026, around 7:00 PM Pacific (6:30-7:30 is acceptable). Aim for about $50 per person for food before tax and tip. Require a reservation without a credit card and free cancellation. Compare up to three options. Check live table availability, menu prices, credit-card requirements, and cancellation terms on the restaurant or booking provider's site. Cite the pages supporting each finding and clearly label anything you cannot verify. Do not infer free cancellation or no-card booking from silence. Do not submit a reservation or enter payment information. Stop with your best verified option and any missing information.

Follow-up:
Take the best verified option to the final booking review for Andrew Nguyen, keeping the same date, party size, budget, and cancellation requirements. Ask me for any missing contact details instead of inventing them. Do not accept fees, enter payment information, opt into marketing, or submit the reservation. Stop before final confirmation and summarize the restaurant, time, party size, name, and exact cancellation terms.

## What to report

- Prompt, decision mode, main model, and elapsed time.
- Whether the task actually completed and any manual intervention.
- Exported step diagnostics, including Qev routing, scores, executions, fallbacks, observation offsets, scrolling movement, and prompt/cache counts.
- Repeated clicks, dropdowns left open, scrolling the sidebar, duplicate text, false verification errors, or needless checks after completion.
