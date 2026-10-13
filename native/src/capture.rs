use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
#[derive(Clone, Debug)]
pub struct Target {
    pub id: u32,
    pub window: bool,
    pub label: String,
}
pub fn targets() -> Result<Vec<Target>> {
    let mut result = Vec::new();
    for m in xcap::Monitor::all()? {
        result.push(Target {
            id: m.id()?,
            window: false,
            label: format!(
                "Monitor: {}",
                m.friendly_name().unwrap_or_else(|_| "Display".into())
            ),
        });
    }
    for w in xcap::Window::all()?.into_iter().take(200) {
        if !w.is_minimized().unwrap_or(true)
            && w.width().unwrap_or(0) > 0
            && w.height().unwrap_or(0) > 0
        {
            let title = w.title().unwrap_or_default();
            if !title.is_empty() {
                result.push(Target {
                    id: w.id()?,
                    window: true,
                    label: format!("Window: {title}"),
                });
            }
        }
    }
    Ok(result)
}
pub fn grab(target: &Target) -> Result<Vec<u8>> {
    let image = if target.window {
        xcap::Window::all()?
            .into_iter()
            .find(|w| w.id().ok() == Some(target.id))
            .context("Window closed")?
            .capture_image()?
    } else {
        xcap::Monitor::all()?
            .into_iter()
            .find(|m| m.id().ok() == Some(target.id))
            .context("Monitor disconnected")?
            .capture_image()?
    };
    anyhow::ensure!(
        image.width() as u64 * image.height() as u64 <= 32_000_000,
        "Screenshot is too large"
    );
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut bytes, image::ImageFormat::Png)?;
    anyhow::ensure!(
        bytes.get_ref().len() <= 32 * 1024 * 1024,
        "Screenshot exceeds size limit"
    );
    Ok(bytes.into_inner())
}
pub fn vision(bytes: &[u8], crop: Option<[u32; 4]>) -> Result<String> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut reader = reader;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut img = reader.decode()?;
    if let Some([x, y, w, h]) = crop {
        anyhow::ensure!(
            w > 0
                && h > 0
                && x < img.width()
                && y < img.height()
                && w <= img.width() - x
                && h <= img.height() - y,
            "Invalid screenshot selection"
        );
        img = img.crop_imm(x, y, w, h);
    }
    img = img.resize(1600, 1600, image::imageops::FilterType::Triangle);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .encode_image(&img.to_rgb8())?;
    anyhow::ensure!(
        jpeg.len() <= 2 * 1024 * 1024,
        "Screenshot is too large to attach"
    );
    Ok(format!("data:image/jpeg;base64,{}", STANDARD.encode(jpeg)))
}
pub fn rgba(bytes: &[u8]) -> Result<image::RgbaImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?.into_rgba8())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screenshot_resize_and_crop() {
        let image = image::RgbaImage::from_pixel(20, 10, image::Rgba([20, 30, 40, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let url = vision(png.get_ref(), Some([2, 2, 10, 5])).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        assert!(vision(png.get_ref(), Some([0, 0, 30, 10])).is_err());
    }
}

#[cfg(windows)]
pub fn select_area() -> Result<Vec<u8>> {
    use windows::{
        Win32::{
            Foundation::*,
            Graphics::Gdi::*,
            System::LibraryLoader::GetModuleHandleW,
            UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
        },
        core::PCWSTR,
    };
    struct Selection {
        start: POINT,
        end: POINT,
        dragging: bool,
        done: bool,
        cancelled: bool,
    }
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe {
            if msg == WM_NCCREATE {
                let create = &*(l.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Selection;
            if ptr.is_null() {
                return DefWindowProcW(hwnd, msg, w, l);
            }
            let s = &mut *ptr;
            match msg {
                WM_LBUTTONDOWN => {
                    let _ = GetCursorPos(&mut s.start);
                    s.end = s.start;
                    s.dragging = true;
                    SetCapture(hwnd);
                    LRESULT(0)
                }
                WM_MOUSEMOVE => {
                    if s.dragging {
                        let _ = GetCursorPos(&mut s.end);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    LRESULT(0)
                }
                WM_LBUTTONUP => {
                    if s.dragging {
                        let _ = GetCursorPos(&mut s.end);
                        s.done = true;
                        let _ = ReleaseCapture();
                        let _ = DestroyWindow(hwnd);
                    }
                    LRESULT(0)
                }
                WM_KEYDOWN if w.0 == 27 => {
                    s.cancelled = true;
                    let _ = DestroyWindow(hwnd);
                    LRESULT(0)
                }
                WM_CLOSE => {
                    s.cancelled = true;
                    let _ = DestroyWindow(hwnd);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                WM_PAINT => {
                    let mut paint = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut paint);
                    let mut client = RECT::default();
                    let _ = GetClientRect(hwnd, &mut client);
                    FillRect(dc, &client, HBRUSH(GetStockObject(BLACK_BRUSH).0));
                    let mut origin = POINT::default();
                    let _ = ClientToScreen(hwnd, &mut origin);
                    let pen = CreatePen(PS_SOLID, 3, COLORREF(0x00FF80A0));
                    let old = SelectObject(dc, HGDIOBJ(pen.0));
                    let brush = SelectObject(dc, GetStockObject(HOLLOW_BRUSH));
                    if s.dragging {
                        let _ = Rectangle(
                            dc,
                            s.start.x.min(s.end.x) - origin.x,
                            s.start.y.min(s.end.y) - origin.y,
                            s.start.x.max(s.end.x) - origin.x,
                            s.start.y.max(s.end.y) - origin.y,
                        );
                    }
                    SelectObject(dc, brush);
                    SelectObject(dc, old);
                    let _ = DeleteObject(HGDIOBJ(pen.0));
                    let _ = EndPaint(hwnd, &paint);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, w, l),
            }
        }
    }
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let name = "VectorDashAreaCapture"
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: PCWSTR(name.as_ptr()),
            hCursor: LoadCursorW(None, IDC_CROSS)?,
            ..Default::default()
        };
        RegisterClassW(&class);
        let mut selection = Box::new(Selection {
            start: POINT::default(),
            end: POINT::default(),
            dragging: false,
            done: false,
            cancelled: false,
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            PCWSTR(name.as_ptr()),
            PCWSTR(name.as_ptr()),
            WS_POPUP,
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
            None,
            None,
            Some(HINSTANCE(instance.0)),
            Some((&mut *selection as *mut Selection).cast()),
        )?;
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 90, LWA_ALPHA)?;
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        anyhow::ensure!(
            selection.done && !selection.cancelled,
            "Screenshot selection cancelled"
        );
        let x = selection.start.x.min(selection.end.x);
        let y = selection.start.y.min(selection.end.y);
        let width = selection.start.x.abs_diff(selection.end.x);
        let height = selection.start.y.abs_diff(selection.end.y);
        anyhow::ensure!(
            width > 0 && height > 0 && width as u64 * height as u64 <= 32_000_000,
            "Invalid capture area"
        );
        std::thread::sleep(std::time::Duration::from_millis(150));
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width as i32, height as i32);
        let old = SelectObject(memory, HGDIOBJ(bitmap.0));
        let result = BitBlt(
            memory,
            0,
            0,
            width as i32,
            height as i32,
            Some(screen),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        );
        SelectObject(memory, old);
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let read = GetDIBits(
            memory,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        result?;
        anyhow::ensure!(read == height as i32, "Could not read screen area");
        for p in pixels.chunks_exact_mut(4) {
            p.swap(0, 2);
            p[3] = 255;
        }
        let image =
            image::RgbaImage::from_raw(width, height, pixels).context("Invalid captured pixels")?;
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut out, image::ImageFormat::Png)?;
        Ok(out.into_inner())
    }
}
#[cfg(not(windows))]
pub fn select_area() -> Result<Vec<u8>> {
    anyhow::bail!("Direct area selection is available on Windows")
}
