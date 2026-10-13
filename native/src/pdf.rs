use anyhow::{Context, Result};
use libloading::Library;
use std::{
    ffi::c_void,
    path::Path,
    sync::{Mutex, OnceLock},
};
type Handle = *mut c_void;
struct Pdfium {
    _library: Library,
    load: unsafe extern "C" fn(*const c_void, usize, *const i8) -> Handle,
    close: unsafe extern "C" fn(Handle),
    count: unsafe extern "C" fn(Handle) -> i32,
    load_page: unsafe extern "C" fn(Handle, i32) -> Handle,
    close_page: unsafe extern "C" fn(Handle),
    width: unsafe extern "C" fn(Handle) -> f64,
    height: unsafe extern "C" fn(Handle) -> f64,
    load_text: unsafe extern "C" fn(Handle) -> Handle,
    close_text: unsafe extern "C" fn(Handle),
    chars: unsafe extern "C" fn(Handle) -> i32,
    unicode: unsafe extern "C" fn(Handle, i32) -> u32,
    bitmap: unsafe extern "C" fn(i32, i32, i32) -> Handle,
    fill: unsafe extern "C" fn(Handle, i32, i32, i32, i32, u32),
    render: unsafe extern "C" fn(Handle, Handle, i32, i32, i32, i32, i32, i32),
    buffer: unsafe extern "C" fn(Handle) -> *mut c_void,
    stride: unsafe extern "C" fn(Handle) -> i32,
    destroy_bitmap: unsafe extern "C" fn(Handle),
    find_start: unsafe extern "C" fn(Handle, *const u16, u32, i32) -> Handle,
    find_next: unsafe extern "C" fn(Handle) -> i32,
    find_index: unsafe extern "C" fn(Handle) -> i32,
    find_count: unsafe extern "C" fn(Handle) -> i32,
    find_close: unsafe extern "C" fn(Handle),
    char_box: unsafe extern "C" fn(Handle, i32, *mut f64, *mut f64, *mut f64, *mut f64) -> i32,
}
unsafe impl Send for Pdfium {}
impl Pdfium {
    unsafe fn new() -> Result<Self> {
        unsafe {
            let path = std::env::var("VECTORDASH_PDFIUM")
                .map(std::path::PathBuf::from)
                .unwrap_or(std::env::current_exe()?.with_file_name("pdfium.dll"));
            let lib = Library::new(path).context("Bundled PDFium renderer is missing")?;
            let init: unsafe extern "C" fn() = *lib.get(b"FPDF_InitLibrary\0")?;
            init();
            macro_rules! get {
                ($n:literal) => {
                    *lib.get(concat!($n, "\0").as_bytes())?
                };
            }
            Ok(Self {
                load: get!("FPDF_LoadMemDocument64"),
                close: get!("FPDF_CloseDocument"),
                count: get!("FPDF_GetPageCount"),
                load_page: get!("FPDF_LoadPage"),
                close_page: get!("FPDF_ClosePage"),
                width: get!("FPDF_GetPageWidth"),
                height: get!("FPDF_GetPageHeight"),
                load_text: get!("FPDFText_LoadPage"),
                close_text: get!("FPDFText_ClosePage"),
                chars: get!("FPDFText_CountChars"),
                unicode: get!("FPDFText_GetUnicode"),
                bitmap: get!("FPDFBitmap_Create"),
                fill: get!("FPDFBitmap_FillRect"),
                render: get!("FPDF_RenderPageBitmap"),
                buffer: get!("FPDFBitmap_GetBuffer"),
                stride: get!("FPDFBitmap_GetStride"),
                destroy_bitmap: get!("FPDFBitmap_Destroy"),
                find_start: get!("FPDFText_FindStart"),
                find_next: get!("FPDFText_FindNext"),
                find_index: get!("FPDFText_GetSchResultIndex"),
                find_count: get!("FPDFText_GetSchCount"),
                find_close: get!("FPDFText_FindClose"),
                char_box: get!("FPDFText_GetCharBox"),
                _library: lib,
            })
        }
    }
}
static PDFIUM: OnceLock<Mutex<Option<Pdfium>>> = OnceLock::new();
fn with_pdf<T>(path: &Path, work: impl FnOnce(&Pdfium, Handle) -> Result<T>) -> Result<T> {
    let bytes = std::fs::read(path)?;
    anyhow::ensure!(bytes.len() <= 128 * 1024 * 1024, "PDF exceeds safe size");
    let mut lock = PDFIUM
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF renderer unavailable"))?;
    if lock.is_none() {
        *lock = Some(unsafe { Pdfium::new()? });
    }
    let pdf = lock.as_ref().unwrap();
    unsafe {
        let document = (pdf.load)(bytes.as_ptr() as *const _, bytes.len(), std::ptr::null());
        anyhow::ensure!(
            !document.is_null(),
            "PDF could not be opened (possibly encrypted or damaged)"
        );
        let result = work(pdf, document);
        (pdf.close)(document);
        result
    }
}
pub fn text(path: &Path) -> Result<Vec<String>> {
    with_pdf(path, |pdf, document| unsafe {
        let count = (pdf.count)(document);
        anyhow::ensure!(count <= 10000, "PDF has too many pages");
        let mut out = vec![];
        for i in 0..count {
            let page = (pdf.load_page)(document, i);
            anyhow::ensure!(!page.is_null(), "PDF page could not be opened");
            let text = (pdf.load_text)(page);
            let count = (pdf.chars)(text);
            let value = (0..count)
                .filter_map(|i| char::from_u32((pdf.unicode)(text, i)))
                .collect::<String>();
            (pdf.close_text)(text);
            (pdf.close_page)(page);
            out.push(value);
        }
        Ok(out)
    })
}
pub fn preview(path: &Path, page_number: u32, query: &str) -> Result<Vec<u8>> {
    with_pdf(path, |pdf, document| unsafe {
        let page = (pdf.load_page)(document, page_number.saturating_sub(1) as i32);
        anyhow::ensure!(!page.is_null(), "Preview page unavailable");
        let width = (pdf.width)(page);
        let height = (pdf.height)(page);
        let w = 1024i32;
        let h = (height / width * 1024.0).clamp(1.0, 4096.0) as i32;
        let bitmap = (pdf.bitmap)(w, h, 0);
        anyhow::ensure!(!bitmap.is_null(), "PDF bitmap allocation failed");
        (pdf.fill)(bitmap, 0, 0, w, h, 0xffffffff);
        (pdf.render)(bitmap, page, 0, 0, w, h, 0, 2);
        let stride = (pdf.stride)(bitmap) as usize;
        let pixels =
            std::slice::from_raw_parts((pdf.buffer)(bitmap) as *const u8, stride * h as usize);
        let mut image = image::RgbImage::new(w as u32, h as u32);
        for (y, row) in pixels.chunks_exact(stride).enumerate() {
            for (x, p) in row[..w as usize * 4].chunks_exact(4).enumerate() {
                image.put_pixel(x as u32, y as u32, image::Rgb([p[2], p[1], p[0]]));
            }
        }
        (pdf.destroy_bitmap)(bitmap);
        let text = (pdf.load_text)(page);
        for word in query.split_whitespace().filter(|w| w.chars().count() >= 3) {
            let token = crate::platform::wide(word);
            let search = (pdf.find_start)(text, token.as_ptr(), 0, 0);
            if search.is_null() {
                continue;
            }
            let mut found = 0;
            while (pdf.find_next)(search) != 0 && found < 100 {
                found += 1;
                let start = (pdf.find_index)(search);
                let len = (pdf.find_count)(search);
                for i in start..start + len {
                    let (mut left, mut right, mut bottom, mut top) = (0.0, 0.0, 0.0, 0.0);
                    if (pdf.char_box)(text, i, &mut left, &mut right, &mut bottom, &mut top) == 0 {
                        continue;
                    }
                    let x0 = (left / width * w as f64).clamp(0.0, (w - 1) as f64) as u32;
                    let x1 = (right / width * w as f64).clamp(0.0, (w - 1) as f64) as u32;
                    let y0 = ((height - top) / height * h as f64).clamp(0.0, (h - 1) as f64) as u32;
                    let y1 =
                        ((height - bottom) / height * h as f64).clamp(0.0, (h - 1) as f64) as u32;
                    for y in y0..=y1 {
                        for x in x0..=x1 {
                            let p = image.get_pixel_mut(x, y);
                            for (c, color) in p.0.iter_mut().zip([133u16, 115, 255]) {
                                *c = ((*c as u16 * 2 + color) / 3) as u8;
                            }
                        }
                    }
                }
            }
            (pdf.find_close)(search);
        }
        (pdf.close_text)(text);
        (pdf.close_page)(page);
        crate::content::jpeg(&image::DynamicImage::ImageRgb8(image), 1024)
    })
}
