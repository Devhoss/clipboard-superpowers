//! Local OCR via built-in Windows.Media.Ocr (PR6).
//!
//! Spike outcome: no new native dependencies. Every Win10+ machine ships
//! the OCR engine (English pack is present on English Windows); tesseract
//! would need a C toolchain + DLLs and ONNX would add tens of MB. No cloud
//! calls, ever — screenshots routinely hold secrets.
//!
//! Async WinRT ops are driven with pure-Rust `block_on`. COM is initialized
//! MTA per call because Tauri commands run on pool threads with no
//! apartment guarantee.

use windows::{
    core::HSTRING,
    Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::StorageFile,
    Win32::{
        Foundation::S_OK,
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    },
};
use std::future::IntoFuture;

/// Extract printed text from an image file. Ok(trimmed text) or an Err
/// naming the missing piece (file, language pack, engine).
pub fn ocr_image_file(path: &str) -> Result<String, String> {
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let ours = hr == S_OK; // S_FALSE = someone else initialized: don't uninit theirs
    let t0 = std::time::Instant::now();
    let result = ocr_inner(path).map_err(|e| e.to_string());
    let ms = t0.elapsed().as_millis();
    if ours {
        unsafe { CoUninitialize() };
    }
    result.map(|t| {
        let text = t.trim().to_string();
        let words = text.split_whitespace().count();
        let rss = peak_rss_mb()
            .map(|mb| format!(", peak RSS {mb:.0} MB"))
            .unwrap_or_default();
        eprintln!("clipboard-superpowers: ocr done in {ms}ms, {words} words{rss}");
        text
    })
}

/// Peak working set of this process, for spike measurements. None when the
/// query itself fails — logging must never break extraction.
fn peak_rss_mb() -> Option<f64> {
    use windows::Win32::System::{ProcessStatus::*, Threading::GetCurrentProcess};
    unsafe {
        let mut pmc = PROCESS_MEMORY_COUNTERS_EX::default();
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut pmc as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
        .ok()?;
        Some(pmc.PeakWorkingSetSize as f64 / 1024.0 / 1024.0)
    }
}

fn ocr_inner(path: &str) -> windows::core::Result<String> {
    // Each WinRT call returns Result<operation>; the operation converts to a
    // future driven to completion with block_on. `run` is generic because
    // every call yields a different operation type.
    fn run<T>(op: windows::core::Result<impl IntoFuture<Output = windows::core::Result<T>>>) -> windows::core::Result<T> {
        futures_executor::block_on(op?.into_future())
    }
    let file = run(StorageFile::GetFileFromPathAsync(&HSTRING::from(path))).map_err(|_| {
        windows::core::Error::new(
            windows::core::HRESULT(-2147024894), // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)
            "image file not found",
        )
    })?;
    let stream = run(file.OpenReadAsync())?;
    let decoder = run(BitmapDecoder::CreateAsync(&stream))?;
    let bitmap = run(decoder.GetSoftwareBitmapAsync())?;
    let gray = SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Gray8)?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| {
        windows::core::Error::new(
            windows::core::HRESULT(-2147467259), // E_FAIL
            "no OCR language available — install English OCR: Settings > Time & language > Language",
        )
    })?;
    let result = run(engine.RecognizeAsync(&gray))?;
    let mut out = String::new();
    for line in result.Lines()? {
        let text = line.Text()?.to_string();
        if !text.trim().is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text.trim());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_a_clean_error() {
        // Error path only: engine creation + accuracy need a real desktop
        // with an OCR language pack, verified manually (see PR body).
        let err = ocr_image_file("E:\\definitely\\not\\here\\x.png").unwrap_err();
        assert!(err.contains("not found"), "unexpected: {err}");
    }
}
