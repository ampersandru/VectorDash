# Computer and browser use — BETA

VectorDash can send approved Windows or browser observations to your existing OpenAI-compatible AI server and execute the model's tool calls locally. The model can run on another PC or server: the Windows PC needs only VectorDash and, for browser tasks, an installed Edge or Chrome. EmbeddingGemma is not the computer-control model.

## Setup

1. In **Models and AI**, configure a chat model supporting OpenAI-style function calls. Use at least **8192 tokens** of context; **16384 or more** is recommended for screenshots or combined tools. Enable vision only if the selected model/server actually supports image input.
2. Enable **Computer control** and/or **Browser control**, then save settings. Their chat toggles appear only when enabled in settings. They begin off.
3. Optionally select an Edge/Chrome executable. Automatic detection uses installed browsers. The optional Microsoft Edge download button opens Microsoft's download page; VectorDash never silently installs a browser, agent runtime, or model.
4. Turn on the corresponding chat toggle and submit a task. Computer can find and launch installed apps by name from the same Windows Start-menu/installed-app catalogue used by VectorDash search. It does not accept arbitrary commands, executable paths or model-supplied launch arguments. Portable apps absent from that catalogue must be opened manually. For browser tasks, supply an HTTP(S) URL.

Browser mode runs in the background by default. **Show browser window** enables a visible dedicated window. Both use a temporary profile, separate from your everyday tabs, extensions, cookies and logins. Computer mode can also inspect and interact with existing browser windows, including their current tabs/logins, subject to its own approval. Use Browser mode when you want the isolated temporary session. Browser file uploads, downloads, system clipboard shortcuts, arbitrary scripts and non-HTTP(S) navigation are not exposed.

Example tasks:

- “Open Notepad.”
- “Open Visual Studio Code.”
- “Use the open browser to go to https://example.com/.”
- “Inspect the open Notepad window and replace its text with this message.”
- “Open this website, find its support link and summarize the page.”
- “Fill the search field on this page, submit it and tell me what changed.”

## Permissions and stopping

Every control call, including reading windows/pages, passes through a host-owned approval gate before the helper runs:

- **Allow once:** execute this one call only.
- **Allow all this session:** allow this particular tool for the current chat. Computer and Browser grants are separate, in memory only.
- **Don't allow:** do not execute the call; finish the response without further control actions.

The prompt identifies the AI server/model, target application or website and relevant input. A new conversation resets both toggles and grants and closes the helper/browser. Changing the model/provider, disabling a tool or stopping control also revokes grants. The existing MCP service remains read-only and does not expose these tools.

**Ctrl+Alt+Backspace** stops computer/browser control by default, including while another application has focus. It can be reassigned under Keyboard shortcuts. The AI Stop button, tool toggles and **Stop tools and revoke permissions** in AI settings also stop control. An already dispatched atomic input action may finish; queued/future work is cancelled. Closing VectorDash terminates its helper/browser process tree and attempts normal temporary-profile cleanup.

Approval is not an OS sandbox. Actions in an approved application can change files or submit information, and untrusted page/window content may try to influence the model. Use **Allow once** while testing. VectorDash prevents its control tools from driving its own approval interface; grants cannot be supplied by the model. Known terminal/password-manager applications and system security dialogs are excluded, and password fields and elevated applications are unsupported.

## Native implementation and resource use

- An on-demand Rust helper uses a private stdin/stdout connection, a Windows kill-on-close job and an MTA accessibility thread. No Python, Node, Hermes, LCU, Playwright service or always-on control listener is installed.
- Enabled privileges are disabled and a high-integrity helper lowers itself to medium integrity before desktop/browser work, even if VectorDash runs elevated for journaling. This is not a general-purpose sandbox.
- Accessibility walks, element counts, screenshots, text and wire messages are bounded. Each action has a 20-second helper timeout; each response has a configurable step budget and a five-minute budget for starting further tool rounds. Human approval occurs before the helper-action timeout starts.
- Windows actions prefer UI Automation or checked Win32 controls. Application discovery returns temporary IDs and friendly names. Only discovered IDs can be launched after approval. Start-menu shortcuts are checked against their resolved executable again before launch; changed targets and known terminal/credential applications are rejected. Approved app launches are handed to Explorer under the normal desktop account; launched apps remain open when AI control stops. They do not inherit the helper’s restricted token or lifetime. The helper and isolated browser are still terminated on Stop/New chat/exit. Filtered window lists include minimized apps and existing browsers; desktop wallpaper and empty activation placeholders are excluded. Packaged-app CoreWindows with background accessibility are marked and cannot receive pixel/key input. Screenshot coordinates are mapped from that individual window image, including negative monitor origins and scaled captures. Fallback input may focus the target and move the real cursor. Windows foreground restrictions can require the user to bring a target forward. Each element action resolves a fresh provider reference and checks its automation identity, type, name, bounds and ancestry before dispatch. Stale elements, moved windows, covered targets and blocked input fail rather than being silently reported as successful.
- Browser control uses typed CDP commands over a random loopback endpoint belonging to its own browser instance. The model cannot supply JavaScript, a CDP endpoint, an executable or a profile path. DOM helpers run in a separate execution world; references are scoped to fresh snapshots.
- Tool calls execute sequentially. Extra parallel calls receive errors, every tool-call ID receives a reply, and actions are never automatically retried after uncertain side effects.
- Screenshots are captured only on request. Older control screenshots are removed from model context; ordinary user image attachments are preserved. Text/accessibility operation works without a vision model.
- Temporary profiles live under VectorDash’s data folder, which scanning and monitoring exclude even when common Windows/cache exclusions are disabled.
- No helper/browser starts while these features are off or before the first approved call. An active browser uses additional Chromium RAM/CPU; stop tools or start a new conversation to release it. Action diagnostics record tool, action, outcome and duration, without page text, typed text or screenshot payloads.

## Testing and limits

`native/scripts/control_native_smoke.py` checks a disposable Win32 window and a local HTTP page: actual text entry, button activation, physical coordinate clicks, screenshots, stale-reference rejection, password/URL restrictions, browser close and per-action timings. Use `--visible` to cover a visible browser as well as the default background mode.

`native/scripts/control_chat_smoke.py` exercises the real AI chat/tool loop against a local OpenAI-compatible fixture server, including once/session/deny, pending approval, new-chat reset, browser DOM changes and screenshot feedback. Its application-launch case discovers Notepad, launches its actual installed entry and checks the running window. It closes only matching windows created during that case. An optional `--case calculator` diagnostic exercises Calculator buttons; packaged-app accessibility/startup can still be unreliable and is not a guarantee of control across all apps. This validates API plumbing and execution, not the reliability of every production model. Automated approval flags are confined to explicit smoke-test runs.

The Rust suite covers consent scope, argument restrictions, parallel-call rejection, multistep remote calls, context bounds and denial handling. Existing screenshot, image search, local search and AI regressions remain relevant.

This first version still needs broader testing on Windows 10, mixed-DPI monitors, complex application accessibility providers and different remote tool/vision models. Browser canvas/coordinate actions, cross-origin frames, browser chrome/extension UI, file transfers, arbitrary scripting and scheduled unattended sessions are outside the initial browser tool surface.

### Local optimized-build measurements

On the disposable fixture on this PC, the updated fresh-reference checks took about 5–8 ms for native text entry and 9–11 ms for a button action; simple window inspection took about 17 ms and screenshots 52–57 ms. Browser startup took about 0.5–0.7 seconds; subsequent DOM operations were about 1 ms. These measure local execution only, excluding model/network latency and human approval. Inspecting the full native accessibility tree of an existing Chromium window took about 3 seconds, so complex apps can be substantially slower; the isolated Browser tool's DOM operations avoid that overhead. Physical mouse input correctly rejected Windows foreground-lock restrictions during unattended runs; an earlier direct fixture run verified a successful coordinate click.

## Reference sources

The implementation is independent; no Hermes or LCU runtime code is bundled. Sources reviewed for the tool/executor separation, consent lifecycle, observations and host protection:

- [Hermes computer-use guide](https://hermes-agent.nousresearch.com/docs/user-guide/features/computer-use), [tool/approval implementation](https://github.com/NousResearch/hermes-agent/blob/main/tools/computer_use/tool.py), [cua-driver bridge](https://github.com/NousResearch/hermes-agent/blob/main/tools/computer_use/cua_backend.py), [browser implementation](https://github.com/NousResearch/hermes-agent/blob/main/tools/browser_tool.py).
- [LCU adapter client and approval forwarding](https://github.com/amontlabs/lcu/blob/main/adapters/client.mjs), [host-app guard](https://github.com/amontlabs/lcu/blob/main/adapters/host-guard.mjs), [current platform support](https://github.com/amontlabs/lcu/blob/main/docs/PARITY-STATUS.md).
- [Microsoft UI Automation patterns](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controlpatternsoverview), [threading guidance](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading), [input integrity restrictions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput).
- [Chrome DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/) and [dedicated debugging-profile requirements](https://developer.chrome.com/blog/remote-debugging-port).
