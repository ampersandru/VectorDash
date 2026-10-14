# Local eframe 0.33.3 patch

Source: the eframe 0.33.3 crates.io package, licensed MIT or Apache-2.0.

Only src/native/run.rs differs: after scheduled redraw deadlines are drained, return to ControlFlow::Wait when no future deadline exists. Windows does not deliver paint events for hidden windows, so leaving ControlFlow::Poll active spins a CPU core until the window becomes visible.

Keep this patch until the upstream dependency fixes the hidden-window case. Verify visible animation, tray idle CPU and hotkey reopening before removing it.
