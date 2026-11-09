//! PDF-Seiten als Bild rendern (für Grundrisse aus PDF-Plänen).
//!
//! Nutzt den in Windows eingebauten PDF-Renderer (`Windows.Data.Pdf`), damit keine zusätzliche
//! Bibliothek mitgeliefert werden muss. Die WinRT-Aufrufe laufen in einem eigenen Thread (MTA),
//! damit `.get()` auf die asynchronen Operationen nicht den UI-Thread blockiert/verklemmt.

use std::path::{Path, PathBuf};

use image::RgbaImage;
use windows::core::HSTRING;
use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
use windows::Storage::StorageFile;
use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

/// Höchstens so viele Seiten bekommen ein Vorschaubild
pub const MAX_THUMBS: usize = 60;

fn in_mta<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        f()
    })
    .join()
    .map_err(|_| "PDF-Renderer abgestürzt".to_string())?
}

fn open(path: &Path) -> Result<PdfDocument, String> {
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))
        .and_then(|op| op.get())
        .map_err(|e| e.message().to_string())?;
    PdfDocument::LoadFromFileAsync(&file).and_then(|op| op.get()).map_err(|e| {
        let msg = e.message().to_string();
        if msg.is_empty() {
            crate::t!("PDF kann nicht geöffnet werden (beschädigt oder mit Passwort geschützt?)", "Cannot open PDF (damaged or password protected?)").into()
        } else {
            msg
        }
    })
}

/// Seite `idx` als PNG rendern, längere Kante = `long_px` Pixel (weißer Hintergrund)
fn render(doc: &PdfDocument, idx: u32, long_px: u32) -> windows::core::Result<Vec<u8>> {
    let page = doc.GetPage(idx)?;
    let size = page.Size()?;
    let (w, h) = (size.Width.max(1.0), size.Height.max(1.0));
    let opts = PdfPageRenderOptions::new()?;
    if w >= h {
        opts.SetDestinationWidth(long_px)?;
    } else {
        opts.SetDestinationHeight(long_px)?;
    }
    let stream = InMemoryRandomAccessStream::new()?;
    page.RenderWithOptionsToStreamAsync(&stream, &opts)?.get()?;
    let len = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    reader.LoadAsync(len)?.get()?;
    let mut buf = vec![0u8; len as usize];
    reader.ReadBytes(&mut buf)?;
    Ok(buf)
}

/// Seitenzahl und Vorschaubilder (höchstens `MAX_THUMBS`)
pub fn thumbnails(path: PathBuf, long_px: u32) -> Result<(u32, Vec<RgbaImage>), String> {
    in_mta(move || {
        let doc = open(&path)?;
        let n = doc.PageCount().map_err(|e| e.message().to_string())?;
        let mut thumbs = Vec::new();
        for i in 0..n.min(MAX_THUMBS as u32) {
            let png = render(&doc, i, long_px).map_err(|e| e.message().to_string())?;
            thumbs.push(image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgba8());
        }
        Ok((n, thumbs))
    })
}

/// Eine Seite in voller Auflösung als PNG
pub fn render_page(path: PathBuf, idx: u32, long_px: u32) -> Result<Vec<u8>, String> {
    in_mta(move || {
        let doc = open(&path)?;
        render(&doc, idx, long_px).map_err(|e| e.message().to_string())
    })
}
