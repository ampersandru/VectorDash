# winit 0.30.13 Windows mixed-DPI dragging fix

During WM_DPICHANGED in the native move loop, use Windows' suggested rectangle directly instead of applying the conservative rectangle and monitor-nudge loop. All non-drag DPI handling remains upstream behavior.

References: https://github.com/rust-windowing/winit/issues/4600 and https://learn.microsoft.com/en-us/windows/win32/hidpi/wm-dpichanged

Remove this override when upgrading to an upstream release containing the fix.
