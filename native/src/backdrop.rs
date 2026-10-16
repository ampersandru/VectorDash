//! A compositor-owned, geometrically clipped host-backdrop. The host is input
//! transparent and has no fill/frame: visually there is only the application's
//! one glass sheet. No desktop images, polling thread or CPU blur is used.
#[cfg(windows)]
mod windows_impl {
    use std::cell::{Cell, RefCell};
    use windows::{
        System::{DispatcherQueue, DispatcherQueueController},
        UI::Composition::{
            CompositionRoundedRectangleGeometry, Compositor, Desktop::DesktopWindowTarget,
            SpriteVisual,
        },
        Win32::{
            Foundation::*,
            Graphics::{Dwm::*, Gdi::*},
            System::{
                LibraryLoader::GetModuleHandleW,
                WinRT::{
                    Composition::ICompositorDesktopInterop, CreateDispatcherQueueController,
                    DQTAT_COM_STA, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
                },
            },
            UI::WindowsAndMessaging::*,
        },
        core::{Interface, w},
    };
    struct Glass {
        hwnd: HWND,
        owner: HWND,
        _queue: Option<DispatcherQueueController>,
        _compositor: Compositor,
        _target: DesktopWindowTarget,
        visual: SpriteVisual,
        geometry: CompositionRoundedRectangleGeometry,
        last: (i32, i32, i32, i32, u32),
    }
    impl Drop for Glass {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
    thread_local! {static FAILED:Cell<bool>=const{Cell::new(false)};}
    thread_local! {static GLASS:RefCell<Option<Glass>>=const{RefCell::new(None)};}
    unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if msg == WM_NCHITTEST {
            return LRESULT(HTTRANSPARENT as isize);
        }
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        unsafe { DefWindowProcW(hwnd, msg, w, l) }
    }
    impl Glass {
        fn new(owner: HWND) -> windows::core::Result<Self> {
            unsafe {
                let queue = if DispatcherQueue::GetForCurrentThread().is_err() {
                    Some(CreateDispatcherQueueController(DispatcherQueueOptions {
                        dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                        threadType: DQTYPE_THREAD_CURRENT,
                        apartmentType: DQTAT_COM_STA,
                    })?)
                } else {
                    None
                };
                let instance = HINSTANCE(GetModuleHandleW(None)?.0);
                let class = w!("VectorDash.ClippedGlass");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(host_proc),
                    hInstance: instance,
                    lpszClassName: class,
                    ..Default::default()
                };
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    WS_EX_TOOLWINDOW
                        | WS_EX_NOACTIVATE
                        | WS_EX_TRANSPARENT
                        | WS_EX_NOREDIRECTIONBITMAP,
                    class,
                    w!(""),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    None,
                    None,
                    Some(instance),
                    None,
                )?;
                let result = (|| {
                    // The blur host must not receive its own DWM border, shadow,
                    // system material or small-radius native corner surface.
                    let policy = DWMNCRP_DISABLED;
                    DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_NCRENDERING_POLICY,
                        &policy as *const _ as _,
                        4,
                    )?;
                    let corners = DWMWCP_DONOTROUND;
                    DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_WINDOW_CORNER_PREFERENCE,
                        &corners as *const _ as _,
                        4,
                    )?;
                    let border = 0xfffffffeu32;
                    DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as _, 4)?;
                    let backdrop = DWMSBT_NONE;
                    DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_SYSTEMBACKDROP_TYPE,
                        &backdrop as *const _ as _,
                        4,
                    )?;
                    let yes = 1i32;
                    DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_USE_HOSTBACKDROPBRUSH,
                        &yes as *const _ as _,
                        4,
                    )?;
                    let compositor = Compositor::new()?;
                    let interop: ICompositorDesktopInterop = compositor.cast()?;
                    let target = interop.CreateDesktopWindowTarget(hwnd, false)?;
                    let visual = compositor.CreateSpriteVisual()?;
                    let brush = compositor.CreateHostBackdropBrush()?;
                    visual.SetBrush(&brush)?;
                    let geometry = compositor.CreateRoundedRectangleGeometry()?;
                    let clip = compositor.CreateGeometricClipWithGeometry(&geometry)?;
                    visual.SetClip(&clip)?;
                    target.SetRoot(&visual)?;
                    Ok(Self {
                        hwnd,
                        owner,
                        _queue: queue,
                        _compositor: compositor,
                        _target: target,
                        visual,
                        geometry,
                        last: (0, 0, 0, 0, 0),
                    })
                })();
                if result.is_err() {
                    let _ = DestroyWindow(hwnd);
                }
                result
            }
        }
        fn sync(&mut self, radius: f32) -> windows::core::Result<()> {
            unsafe {
                if !IsWindowVisible(self.owner).as_bool() || IsIconic(self.owner).as_bool() {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                    return Ok(());
                }
                let mut rect = RECT::default();
                GetClientRect(self.owner, &mut rect)?;
                let width = rect.right;
                let height = rect.bottom;
                let mut origin = POINT::default();
                if !ClientToScreen(self.owner, &mut origin).as_bool() {
                    return Err(windows::core::Error::from_win32());
                }
                rect.left = origin.x;
                rect.top = origin.y;
                let rounded = if IsZoomed(self.owner).as_bool() {
                    0.0
                } else {
                    radius
                };
                let key = (rect.left, rect.top, width, height, rounded.to_bits());
                let unchanged = self.last == key;
                if !unchanged {
                    let size = windows_numerics::Vector2 {
                        X: width as f32,
                        Y: height as f32,
                    };
                    self.visual.SetSize(size)?;
                    self.geometry.SetSize(size)?;
                    self.geometry.SetCornerRadius(windows_numerics::Vector2 {
                        X: rounded,
                        Y: rounded,
                    })?;
                    // Clip the HWND as well as its composition visual. Otherwise Windows
                    // can composite a rectangular host surface outside the glass curve.
                    // Leave the GPU geometric clip's antialiased pixels inside
                    // the hard HWND mask; the clip still owns the visible radius.
                    let diameter = ((rounded - 2.0).max(0.0) * 2.0).round() as i32;
                    let region = if diameter > 0 {
                        CreateRoundRectRgn(0, 0, width, height, diameter, diameter)
                    } else {
                        CreateRectRgn(0, 0, width, height)
                    };
                    if SetWindowRgn(self.hwnd, Some(region), true) == 0 {
                        let _ = DeleteObject(region.into());
                        return Err(windows::core::Error::from_win32());
                    }
                    self.last = key;
                }
                if unchanged
                    && IsWindowVisible(self.hwnd).as_bool()
                    && GetWindow(self.hwnd, GW_HWNDPREV).is_ok_and(|window| window == self.owner)
                    && (GetWindowLongPtrW(self.owner, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize)
                        == (GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize)
                {
                    return Ok(());
                }
                // Follow quick search into/out of the topmost band without activation.
                let owner_top =
                    GetWindowLongPtrW(self.owner, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0;
                let host_top =
                    GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0;
                if owner_top != host_top {
                    SetWindowPos(
                        self.hwnd,
                        Some(if owner_top {
                            HWND_TOPMOST
                        } else {
                            HWND_NOTOPMOST
                        }),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    )?;
                }
                // Immediately below the UI, never above it or activated by pointer input.
                SetWindowPos(
                    self.hwnd,
                    Some(self.owner),
                    rect.left,
                    rect.top,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )?;
                Ok(())
            }
        }
    }
    pub fn sync(hwnd: isize, radius: f32) -> bool {
        // Enabling a host backdrop on a Win32 window requires the Windows 11 DWM
        // attribute. Keep the rounded, tinted UI on Windows 10 rather than creating
        // a black or rectangular host surface through unsupported attributes.
        if crate::platform::windows_build() < 22000 {
            return false;
        }
        if crate::platform::development_overrides()
            && std::env::var_os("VECTORDASH_TEST_NO_GLASS").is_some()
        {
            return false;
        }
        if FAILED.with(Cell::get) {
            return false;
        }
        GLASS.with(|slot| {
            let Ok(mut slot) = slot.try_borrow_mut() else {
                return false;
            };
            if slot.as_ref().is_some_and(|g| g.owner.0 as isize != hwnd) {
                slot.take();
            }
            if slot.is_none() {
                match Glass::new(HWND(hwnd as *mut _)) {
                    Ok(g) => *slot = Some(g),
                    Err(error) => {
                        FAILED.with(|failed| failed.set(true));
                        eprintln!("Clipped glass initialization: {error}");
                        return false;
                    }
                }
            }
            slot.as_mut().unwrap().sync(radius).is_ok()
        })
    }
    pub fn follow(hwnd: isize) {
        GLASS.with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                if let Some(glass) = slot.as_mut() {
                    if glass.owner.0 as isize == hwnd {
                        let radius = f32::from_bits(glass.last.4);
                        let _ = glass.sync(radius);
                    }
                }
            }
        });
    }
    pub fn hide() {
        GLASS.with(|slot| {
            if let Ok(slot) = slot.try_borrow() {
                if let Some(glass) = slot.as_ref() {
                    unsafe {
                        let _ = ShowWindow(glass.hwnd, SW_HIDE);
                    }
                }
            }
        });
    }
}
#[cfg(windows)]
pub use windows_impl::{follow, hide, sync};
#[cfg(not(windows))]
pub fn sync(_: isize, _: f32) -> bool {
    false
}
#[cfg(not(windows))]
pub fn hide() {}

#[cfg(not(windows))]
pub fn follow(_: isize) {}
