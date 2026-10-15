//! Windows video frames through Media Foundation. Readers are scoped to one file;
//! only the sampled frame is copied/encoded, never the whole video.
use crate::config::Settings;
use anyhow::{Context, Result};
use std::path::Path;
pub fn gpu_requested(s: &Settings) -> bool {
    s.video_decode_device == "GPU"
        || (s.video_decode_device == "Auto" && s.hardware != "CPU / system RAM")
}
pub fn ffmpeg_device(command: &mut std::process::Command, s: &Settings) {
    command.args(["-threads", "2", "-filter_threads", "1"]);
    if !gpu_requested(s) {
        command.args(["-hwaccel", "none"]);
    } else if !s.video_gpu.is_empty() {
        command.args(["-hwaccel", "d3d11va", "-hwaccel_device", &s.video_gpu]);
    } else {
        command.args(["-hwaccel", "auto"]);
    }
}
#[cfg(windows)]
mod win {
    use super::*;
    use windows::{
        Win32::{
            Graphics::{Direct3D::*, Direct3D11::*, Dxgi::*},
            Media::MediaFoundation::*,
            System::Com::{
                COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize,
                StructuredStorage::PROPVARIANT,
            },
        },
        core::{GUID, Interface, PCWSTR},
    };
    struct Platform {
        com: bool,
        mf: bool,
        modules: Vec<windows::Win32::Foundation::HMODULE>,
    }
    impl Platform {
        fn start() -> Result<Self> {
            unsafe {
                let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
                let mut platform = Self {
                    com: hr.is_ok(),
                    mf: false,
                    modules: vec![],
                };
                if hr.is_err() && hr.0 != 0x80010106u32 as i32 {
                    hr.ok()?;
                }
                use windows::Win32::System::LibraryLoader::{
                    LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
                };
                for library in [
                    windows::core::w!("mfplat.dll"),
                    windows::core::w!("mfreadwrite.dll"),
                ] {
                    platform.modules.push(LoadLibraryExW(
                        library,
                        None,
                        LOAD_LIBRARY_SEARCH_SYSTEM32,
                    )?);
                }
                MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
                platform.mf = true;
                Ok(platform)
            }
        }
    }
    impl Drop for Platform {
        fn drop(&mut self) {
            unsafe {
                if self.mf {
                    let _ = MFShutdown();
                }
                if self.com {
                    CoUninitialize();
                }
                for module in self.modules.drain(..).rev() {
                    let _ = windows::Win32::Foundation::FreeLibrary(module);
                }
            }
        }
    }
    pub fn adapters() -> Vec<(String, String)> {
        static CACHE: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
        CACHE
            .get_or_init(|| unsafe {
                let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
                    return vec![];
                };
                (0..16)
                    .map_while(|i| factory.EnumAdapters1(i).ok().map(|a| (i, a)))
                    .filter_map(|(i, a)| {
                        let d = a.GetDesc1().ok()?;
                        if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                            return None;
                        }
                        let n = d
                            .Description
                            .iter()
                            .position(|v| *v == 0)
                            .unwrap_or(d.Description.len());
                        Some((i.to_string(), String::from_utf16_lossy(&d.Description[..n])))
                    })
                    .collect()
            })
            .clone()
    }
    pub struct Reader {
        source: IMFSourceReader,
        _manager: Option<IMFDXGIDeviceManager>,
        _platform: Platform,
    }
    impl Reader {
        pub fn open(path: &Path, s: &Settings) -> Result<Self> {
            unsafe {
                let platform = Platform::start()?;
                let mut attrs = None;
                MFCreateAttributes(&mut attrs, 5)?;
                let attrs = attrs.context("Video attributes unavailable")?;
                attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
                attrs.SetUINT32(&MF_LOW_LATENCY, 1)?;
                let manager = if gpu_requested(s) {
                    let factory = CreateDXGIFactory1::<IDXGIFactory1>()?;
                    let adapter = if let Ok(index) = s.video_gpu.parse::<u32>() {
                        Some(factory.EnumAdapters1(index)?.cast::<IDXGIAdapter>()?)
                    } else {
                        None
                    };
                    let mut device = None;
                    D3D11CreateDevice(
                        adapter.as_ref(),
                        if adapter.is_some() {
                            D3D_DRIVER_TYPE_UNKNOWN
                        } else {
                            D3D_DRIVER_TYPE_HARDWARE
                        },
                        windows::Win32::Foundation::HMODULE::default(),
                        D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                        None,
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        None,
                    )?;
                    let device = device.context("Video GPU unavailable")?;
                    // Media Foundation invokes the D3D device from its own threads.
                    // Protect the immediate context or hardware transforms can deadlock.
                    let context = device.GetImmediateContext()?;
                    let multithread = context
                        .cast::<windows::Win32::Graphics::Direct3D10::ID3D10Multithread>(
                    )?;
                    let _ = multithread.SetMultithreadProtected(true);

                    let mut token = 0;
                    let mut manager = None;
                    MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
                    let manager = manager.context("Video device manager unavailable")?;
                    manager.ResetDevice(&device, token)?;
                    attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &manager)?;
                    attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
                    Some(manager)
                } else {
                    attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 0)?;
                    None
                };
                let url = path
                    .as_os_str()
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<_>>();
                let source = MFCreateSourceReaderFromURL(PCWSTR(url.as_ptr()), &attrs)?;
                source.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
                source.SetStreamSelection(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, true)?;
                let ty = MFCreateMediaType()?;
                ty.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                ty.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
                // Request the embedding thumbnail size from Windows' video processor,
                // avoiding full-resolution RGB buffer pools and a second CPU resize.
                let native =
                    source.GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, 0)?;
                let size = native.GetUINT64(&MF_MT_FRAME_SIZE)?;
                let (w, h) = ((size >> 32) as u32, size as u32);
                if w.max(h) > 1024 {
                    let scale = 1024.0 / w.max(h) as f64;
                    let width = ((w as f64 * scale).round() as u32).max(2) & !1;
                    let height = ((h as f64 * scale).round() as u32).max(2) & !1;
                    ty.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
                }
                if source
                    .SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, None, &ty)
                    .is_err()
                {
                    // Some Windows codecs cannot scale in the source reader.
                    ty.DeleteItem(&MF_MT_FRAME_SIZE)?;
                    source.SetCurrentMediaType(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        None,
                        &ty,
                    )?;
                }
                Ok(Self {
                    source,
                    _manager: manager,
                    _platform: platform,
                })
            }
        }
        pub fn has_audio(&self) -> bool {
            unsafe {
                self.source
                    .GetNativeMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32, 0)
                    .is_ok()
            }
        }
        pub fn duration(&self) -> Option<f32> {
            unsafe {
                let value = self
                    .source
                    .GetPresentationAttribute(
                        MF_SOURCE_READER_MEDIASOURCE.0 as u32,
                        &MF_PD_DURATION,
                    )
                    .ok()?;
                i64::try_from(&value).ok().map(|v| v as f32 / 10_000_000.0)
            }
        }
        pub fn frame(
            &self,
            seconds: f32,
            checkpoint: &dyn Fn() -> Result<()>,
        ) -> Result<Option<(f32, Vec<u8>)>> {
            unsafe {
                checkpoint()?;
                let target = (seconds.max(0.0) as f64 * 10_000_000.0) as i64;
                self.source
                    .SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from(target))?;
                for _ in 0..100000 {
                    checkpoint()?;
                    let mut flags = 0;
                    let mut stamp = 0;
                    let mut sample = None;
                    self.source.ReadSample(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut stamp),
                        Some(&mut sample),
                    )?;
                    if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                        return Ok(None);
                    }
                    if stamp < target {
                        continue;
                    }
                    let Some(sample) = sample else { continue };
                    let ty = self
                        .source
                        .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)?;
                    let size = ty.GetUINT64(&MF_MT_FRAME_SIZE)?;
                    let (w, h) = ((size >> 32) as u32, size as u32);
                    anyhow::ensure!(
                        w > 0 && h > 0 && (w as u64 * h as u64) <= 16_777_216,
                        "Native video frame exceeds size limit"
                    );
                    let buffer = sample.ConvertToContiguousBuffer()?;
                    let stride = ty.GetUINT32(&MF_MT_DEFAULT_STRIDE).unwrap_or(w * 4) as i32;
                    let mut ptr = std::ptr::null_mut();
                    let mut len = 0;
                    buffer.Lock(&mut ptr, None, Some(&mut len))?;
                    struct Lock<'a>(&'a IMFMediaBuffer);
                    impl Drop for Lock<'_> {
                        fn drop(&mut self) {
                            unsafe {
                                let _ = self.0.Unlock();
                            }
                        }
                    }
                    let _lock = Lock(&buffer);
                    let pitch = stride.unsigned_abs() as usize;
                    anyhow::ensure!(
                        !ptr.is_null()
                            && pitch >= w as usize * 4
                            && pitch * (h as usize - 1) + w as usize * 4 <= len as usize,
                        "Invalid native video buffer"
                    );
                    let bytes = std::slice::from_raw_parts(ptr, len as usize);
                    let mut image = image::RgbImage::new(w, h);
                    for y in 0..h as usize {
                        let row = if stride < 0 { h as usize - 1 - y } else { y };
                        for x in 0..w as usize {
                            let p = row * pitch + x * 4;
                            image.put_pixel(
                                x as u32,
                                y as u32,
                                image::Rgb([bytes[p + 2], bytes[p + 1], bytes[p]]),
                            );
                        }
                    }
                    let data = crate::content::jpeg(
                        &image::DynamicImage::ImageRgb8(image),
                        1024.min(w.max(h)),
                    )?;
                    return Ok(Some((stamp as f32 / 10_000_000.0, data)));
                }
                anyhow::bail!("Native video seek limit reached")
            }
        }
    }
}
#[cfg(windows)]
pub use win::{Reader, adapters};
#[cfg(not(windows))]
pub fn adapters() -> Vec<(String, String)> {
    vec![]
}
#[cfg(not(windows))]
pub struct Reader;
#[cfg(not(windows))]
impl Reader {
    pub fn open(_: &Path, _: &Settings) -> Result<Self> {
        anyhow::bail!("Windows video decoder is unavailable on this platform")
    }
    pub fn duration(&self) -> Option<f32> {
        None
    }
    pub fn has_audio(&self) -> bool {
        false
    }
    pub fn frame(&self, _: f32, _: &dyn Fn() -> Result<()>) -> Result<Option<(f32, Vec<u8>)>> {
        anyhow::bail!("Windows video decoder is unavailable")
    }
}
pub fn open(path: &Path, s: &Settings) -> Result<Reader> {
    match Reader::open(path, s) {
        Ok(r) => Ok(r),
        Err(_) if s.video_decode_device == "Auto" && gpu_requested(s) => {
            let mut cpu = s.clone();
            cpu.video_decode_device = "CPU".into();
            Reader::open(path, &cpu)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoder_device_is_separate_from_embedding_device() {
        let mut s = Settings::default();
        s.hardware = "CPU / system RAM".into();
        assert!(!gpu_requested(&s));
        s.video_decode_device = "GPU".into();
        assert!(gpu_requested(&s));
        s.hardware = "Dedicated GPU".into();
        s.video_decode_device = "CPU".into();
        assert!(!gpu_requested(&s));
        s.video_decode_device = "Auto".into();
        assert!(gpu_requested(&s));
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "Requires Windows Media Foundation H.264 decoder"]
    fn native_cpu_decodes_seeks_and_releases_without_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("colors.mp4");
        std::fs::write(&path, include_bytes!("../tests/fixtures/video-colors.mp4")).unwrap();
        let s = Settings {
            video_decode_device: "CPU".into(),
            ..Default::default()
        };
        let reader = open(&path, &s).unwrap();
        assert!(!reader.has_audio());
        assert!(reader.duration().unwrap() > 1.9);
        for time in [0.0, 1.0] {
            let (stamp, jpeg) = reader.frame(time, &|| Ok(())).unwrap().unwrap();
            assert!(stamp >= time && stamp < time + 0.6);
            let image = image::load_from_memory(&jpeg).unwrap().to_rgb8();
            let top = image.get_pixel(image.width() / 2, image.height() / 4);
            let bottom = image.get_pixel(image.width() / 2, image.height() * 3 / 4);
            assert!(
                top[0] > 200 && top[2] < 40,
                "RGB channel order/top orientation"
            );
            assert!(bottom[2] > 200 && bottom[0] < 40, "bottom orientation");
        }
        assert!(
            reader
                .frame(0.0, &|| anyhow::bail!("cancelled"))
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
}
