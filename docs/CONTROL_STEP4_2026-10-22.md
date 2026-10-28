# Explorer copy workflow regression — 2026-10-22

The supplied conversation reached a failure in its fourth request (job 12). Creating Archive left the new-folder inline rename editor active. The next Ctrl+C/Ctrl+V/Delete actions acted in that editor rather than copying the separately observed plan.txt file. A subsequent type targeting the plan.txt list row changed its writable UIA value to plan-backup.txt. The model then cut and moved it into Archive. The parent folder reported only Archive, so the requested original-plus-copy invariant was not met. Later inspection/identity failures consumed the remaining rounds; the final diagnostic was incomplete, not success.

Fixes:

- Native typing requires Edit or Document controls before any native or UIA Value write. Writable ListItem values cannot be used to implicitly rename files. Renaming requires an actual editor, for example select the item, F2, inspect, type and Enter.
- Inline rename fields reject Copy/Cut/Paste/Delete with an explanation and no keystroke dispatch. The guard recognizes a ListItem parent or Explorer's inline rename editor identity; it does not disable clipboard input in normal document/search editors.
- Successful list selection also sets keyboard focus. Text-entry feedback distinguishes changing the editor from committing a rename/navigation. Instructions require a distinct duplicate before renaming/moving the duplicate, preserving the original.
- F5 refresh and Ctrl+E search are supported alongside existing keyboard navigation.

The supplied user files were not modified during diagnosis or testing. Do not assume plan.txt remains in the parent folder after the failed run: inspect Archive/plan-backup.txt and the parent before any manual recovery. Use a new uniquely named folder for retesting.

Validation uses disposable Win32 controls and localhost browser fixtures, plus unit coverage. Native typing into a non-editor is rejected before changing a value, normal editor text operations still pass, and Chrome navigation/typing/click/stale-target checks pass. The default detected browser exited before exposing its control endpoint in the first fixture attempt; the explicit Chrome retry passed. Physical foreground pointer input was safely refused by Windows in the test session, so this is not a claim that the full Explorer copy workflow was automatically verified. An interactive retest is still needed.
