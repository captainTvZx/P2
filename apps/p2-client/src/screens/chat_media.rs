//! Sohbet medyası (L ajanı) — MessageRichText.tsx / AttachmentViewer.tsx'in Electron'da tarayıcıya
//! bıraktığı işler:
//! - bağlantı önizlemesi: `RemoteImagePreview` (bağlantı doğrudan resim olarak istenir; resim değilse
//!   gizli kalır) ve `YoutubeCard` (i.ytimg.com küçük resmi). Electron'un yaptığından başka üçüncü
//!   taraf isteği yok: aynı adresler, istemci tarafında.
//! - PDF: pdf.js yerine Windows'un yerleşik PDF çizicisi (`Windows.Data.Pdf`); ek indirme yok.
//! - video/ses: `<video>`/`<audio>` yerine Media Foundation kaynak okuyucusu (RGB32 kare + float ses).
//!   Ses yalnız `P2_NO_AUDIO_DEVICE` YOKKEN hoparlöre gider (p2-media `AudioIo`); testlerde saat duvar
//!   saatiyle ilerler, hiçbir ses aygıtı açılmaz.
//! - arşiv/DOCX/sunum/tablo: ZIP merkez dizini elle okunur; DOCX/PPTX/XLSX metni XML'den çıkarılır.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::RenderImage;

// ---------------------------------------------------------------- bağlantılar

/// `https?://[^\s]+` (MessageRichText `urls`), sırası korunur, tekrarsız.
pub fn urls(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split(char::is_whitespace) {
        let mut rest = word;
        while let Some(at) = rest.find("http://").into_iter().chain(rest.find("https://")).min() {
            let tail = &rest[at..];
            let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
            let url = &tail[..end];
            if url.len() > "https://".len() && !out.iter().any(|u| u == url) {
                out.push(url.to_string());
            }
            rest = &tail[end..];
            if rest.is_empty() {
                break;
            }
            // Aynı sözcükte ikinci adres yoksa çık.
            if !rest.contains("http") {
                break;
            }
            rest = &rest[1..];
        }
    }
    out
}

/// MessageRichText `youtubeId`: youtu.be/<id>, *.youtube.com/watch?v=<id>, /shorts/<id>.
pub fn youtube_id(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((rest, "/".into()));
    let host = host.split('@').next_back()?.split(':').next()?.to_ascii_lowercase();
    let (path, query) = path.split_once('?').map(|(p, q)| (p.to_string(), q.split('#').next().unwrap_or("").to_string())).unwrap_or((path.split('#').next().unwrap_or("").to_string(), String::new()));
    if host == "youtu.be" {
        let id = path.trim_start_matches('/').split('/').next().unwrap_or("");
        return Some(id.to_string());
    }
    if host.ends_with("youtube.com") {
        if let Some(v) = query.split('&').find_map(|kv| kv.strip_prefix("v=")).filter(|v| !v.is_empty()) {
            return Some(v.to_string());
        }
        if let Some(id) = path.strip_prefix("/shorts/").map(|s| s.split('/').next().unwrap_or("")).filter(|s| !s.is_empty()) {
            return Some(id.to_string());
        }
    }
    None
}

/// Metindeki YouTube kimlikleri (tekrarsız, sırayla).
pub fn youtube_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in urls(text).iter().filter_map(|u| youtube_id(u)) {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// MessageRichText `RemoteImagePreview`: Electron'da `loading="lazy"` + `display:none` (yüklenene dek)
/// birleşimi yüzünden resim HİÇ istenmez ve önizleme görünmez (Chromium gizli tembel resmi yüklemez).
/// Birebir eşdeğerlik için Rust da istemez; Electron düzeltilirse bu bayrak açılır.
pub const REMOTE_IMAGE_PREVIEW: bool = false;

fn http() -> &'static reqwest::blocking::Client {
    static CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| reqwest::blocking::Client::builder().timeout(Duration::from_secs(15)).connect_timeout(Duration::from_secs(8)).build().expect("HTTP istemcisi"))
}

/// Uzak resmi indirir (Electron `<img src=url>` ile aynı istek); resim değilse `None`.
/// En çok 20 MB okunur.
pub fn fetch_remote_image(url: &str) -> Option<(Arc<RenderImage>, u32, u32)> {
    use std::io::Read as _;
    let resp = http().get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let mut data = Vec::new();
    resp.take(20 * 1024 * 1024).read_to_end(&mut data).ok()?;
    decode_full(&data, 1120)
}

/// `decode_image` gibi; en uzun kenar `max` px (önizleme 560 px genişlikte çizilir).
pub fn decode_full(bytes: &[u8], max: u32) -> Option<(Arc<RenderImage>, u32, u32)> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let decoded = if decoded.width() > max || decoded.height() > max { decoded.thumbnail(max, max) } else { decoded };
    Some(bgra_image(decoded.to_rgba8()))
}

fn bgra_image(mut buf: image::RgbaImage) -> (Arc<RenderImage>, u32, u32) {
    let (w, h) = (buf.width(), buf.height());
    for px in buf.pixels_mut() {
        px.0.swap(0, 2);
    }
    (Arc::new(RenderImage::new(vec![image::Frame::new(buf)])), w, h)
}

// ---------------------------------------------------------------- PDF (Windows.Data.Pdf)

pub struct PdfPages {
    pub count: u32,
    /// (görüntü, piksel genişlik, piksel yükseklik, sayfa noktası genişlik, yükseklik)
    pub pages: Vec<(Arc<RenderImage>, u32, u32, f32, f32)>,
}

/// PDF'i belleğe alır ve `pages` kadar sayfayı `scale` (pdf.js viewport ölçeği: 1 = 72 dpi nokta
/// başına 1 px) × `dpr` ile çizer.
pub fn render_pdf(bytes: &[u8], pages: usize, scale: f32) -> Result<PdfPages, String> {
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};
    let err = |e: windows::core::Error| e.message().to_string();
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
    }
    let input = InMemoryRandomAccessStream::new().map_err(err)?;
    let writer = DataWriter::CreateDataWriter(&input).map_err(err)?;
    writer.WriteBytes(bytes).map_err(err)?;
    writer.StoreAsync().map_err(err)?.join().map_err(err)?;
    writer.FlushAsync().map_err(err)?.join().map_err(err)?;
    let _ = writer.DetachStream();
    input.Seek(0).map_err(err)?;
    let doc = PdfDocument::LoadFromStreamAsync(&input).map_err(err)?.join().map_err(err)?;
    let count = doc.PageCount().map_err(err)?;
    let mut out = Vec::new();
    for ix in 0..(pages as u32).min(count) {
        let page = doc.GetPage(ix).map_err(err)?;
        let size = page.Size().map_err(err)?;
        // Size: DIP (1/96 inç) → PDF noktası (1/72 inç).
        let (pw, ph) = (size.Width * 72. / 96., size.Height * 72. / 96.);
        let (w, h) = ((pw * scale).floor().max(1.) as u32, (ph * scale).floor().max(1.) as u32);
        let opts = PdfPageRenderOptions::new().map_err(err)?;
        opts.SetDestinationWidth(w).map_err(err)?;
        opts.SetDestinationHeight(h).map_err(err)?;
        let stream = InMemoryRandomAccessStream::new().map_err(err)?;
        page.RenderWithOptionsToStreamAsync(&stream, &opts).map_err(err)?.join().map_err(err)?;
        let len = stream.Size().map_err(err)? as u32;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(err)?).map_err(err)?;
        reader.LoadAsync(len).map_err(err)?.join().map_err(err)?;
        let mut png = vec![0u8; len as usize];
        reader.ReadBytes(&mut png).map_err(err)?;
        let decoded = image::load_from_memory(&png).map_err(|e| e.to_string())?;
        // PDF çizicisi saydam zemin bırakabilir: canvas gibi beyaza bindir.
        let mut rgba = decoded.to_rgba8();
        for px in rgba.pixels_mut() {
            let a = px.0[3] as u32;
            if a < 255 {
                for c in 0..3 {
                    px.0[c] = ((px.0[c] as u32 * a + 255 * (255 - a)) / 255) as u8;
                }
                px.0[3] = 255;
            }
        }
        let (image, iw, ih) = bgra_image(rgba);
        out.push((image, iw, ih, pw, ph));
    }
    Ok(PdfPages { count, pages: out })
}

// ---------------------------------------------------------------- ZIP (arşiv listesi, ofis metni)

pub struct ZipEntry {
    pub name: String,
    pub size: u64,
    method: u16,
    csize: u64,
    offset: u64,
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Merkez dizini okur (çıkarma yapmadan, ArchiveViewer gibi en çok 1000 girdi).
pub fn zip_entries(data: &[u8]) -> Option<Vec<ZipEntry>> {
    let tail = data.len().saturating_sub(65_557);
    let eocd = (tail..data.len().saturating_sub(21)).rev().find(|&i| data[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]))?;
    let total = u16le(data, eocd + 10)? as usize;
    let mut at = u32le(data, eocd + 16)? as usize;
    let mut out = Vec::new();
    for _ in 0..total.min(1000) {
        if !data.get(at..)?.starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            break;
        }
        let method = u16le(data, at + 10)?;
        let csize = u32le(data, at + 20)? as u64;
        let size = u32le(data, at + 24)? as u64;
        let (nl, el, cl) = (u16le(data, at + 28)? as usize, u16le(data, at + 30)? as usize, u16le(data, at + 32)? as usize);
        let offset = u32le(data, at + 42)? as u64;
        let name = String::from_utf8_lossy(data.get(at + 46..at + 46 + nl)?).into_owned();
        out.push(ZipEntry { name, size, method, csize, offset });
        at += 46 + nl + el + cl;
    }
    Some(out)
}

/// Bir girdiyi açar (saklanmış ya da deflate); 5 MB sınırı (AttachmentViewer `safeZipText`).
pub fn zip_read(data: &[u8], entry: &ZipEntry) -> Option<Vec<u8>> {
    if entry.size > 5 * 1024 * 1024 {
        return None;
    }
    let at = entry.offset as usize;
    if !data.get(at..)?.starts_with(&[0x50, 0x4b, 0x03, 0x04]) {
        return None;
    }
    let (nl, el) = (u16le(data, at + 26)? as usize, u16le(data, at + 28)? as usize);
    let start = at + 30 + nl + el;
    let raw = data.get(start..start + entry.csize as usize)?;
    match entry.method {
        0 => Some(raw.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, 5 * 1024 * 1024).ok(),
        _ => None,
    }
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// `<tag ...>metin</tag>` içeriklerini sırayla toplar (ad alanlı etiketler: `w:t`, `a:t`, `t`).
fn tag_texts(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        // `<w:tab/>` gibi başka etiketle karışmasın: ardından boşluk ya da '>' gelmeli.
        if !after.starts_with('>') && !after.starts_with(' ') {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find(&close) else { break };
        out.push(xml_unescape(&body[..end]));
        rest = &body[end + close.len()..];
    }
    out
}

/// Belge önizlemesinin bölümü: başlık (isteğe bağlı) ve paragraflar / tablo satırları.
pub struct OfficeSection {
    pub title: Option<String>,
    pub paragraphs: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// DOCX (`word/document.xml` paragrafları), PPTX (slayt metinleri), XLSX (sayfa hücreleri), ODF (`content.xml`).
pub fn office_sections(data: &[u8], name: &str) -> Result<Vec<OfficeSection>, String> {
    let fail = || p2_i18n::t("chat.doc.previewFailed");
    let entries = zip_entries(data).ok_or_else(fail)?;
    let total: u64 = entries.iter().map(|e| e.size).sum();
    if total > 100 * 1024 * 1024 {
        return Err(fail());
    }
    let text = |path: &str| entries.iter().find(|e| e.name == path).and_then(|e| zip_read(data, e)).map(|b| String::from_utf8_lossy(&b).into_owned());
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".docx") {
        let xml = text("word/document.xml").ok_or_else(fail)?;
        let paragraphs = xml.split("</w:p>").map(|p| tag_texts(p, "w:t").concat()).filter(|p| !p.trim().is_empty()).collect();
        return Ok(vec![OfficeSection { title: None, paragraphs, rows: Vec::new() }]);
    }
    if lower.ends_with(".pptx") {
        let mut slides: Vec<&ZipEntry> = entries.iter().filter(|e| e.name.starts_with("ppt/slides/slide") && e.name.ends_with(".xml") && !e.name[16..].contains('/')).collect();
        slides.sort_by_key(|e| e.name.trim_start_matches("ppt/slides/slide").trim_end_matches(".xml").parse::<u32>().unwrap_or(0));
        return Ok(slides
            .iter()
            .take(200)
            .enumerate()
            .map(|(i, e)| {
                let xml = zip_read(data, e).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                OfficeSection { title: Some(p2_i18n::tv("chat.doc.slide", &[("n", &(i + 1).to_string())])), paragraphs: vec![tag_texts(&xml, "a:t").join(" · ")], rows: Vec::new() }
            })
            .collect());
    }
    if lower.ends_with(".xlsx") {
        let shared: Vec<String> = text("xl/sharedStrings.xml").map(|x| x.split("</si>").map(|si| tag_texts(si, "t").concat()).collect()).unwrap_or_default();
        let mut sheets: Vec<&ZipEntry> = entries.iter().filter(|e| e.name.starts_with("xl/worksheets/sheet") && e.name.ends_with(".xml")).collect();
        sheets.sort_by_key(|e| e.name.trim_start_matches("xl/worksheets/sheet").trim_end_matches(".xml").parse::<u32>().unwrap_or(0));
        let out: Vec<OfficeSection> = sheets
            .iter()
            .take(20)
            .enumerate()
            .map(|(i, e)| {
                let xml = zip_read(data, e).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                let rows = xml
                    .split("</row>")
                    .filter(|r| r.contains("<row"))
                    .take(200)
                    .map(|row| {
                        row.split("</c>")
                            .filter(|c| c.contains("<c "))
                            .take(100)
                            .map(|cell| {
                                let raw = tag_texts(cell, "v").concat();
                                if cell.contains("t=\"s\"") { raw.parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or(raw) } else { raw }
                            })
                            .collect()
                    })
                    .collect();
                OfficeSection { title: Some(p2_i18n::tv("chat.doc.sheet", &[("n", &(i + 1).to_string())])), paragraphs: Vec::new(), rows }
            })
            .collect();
        return if out.is_empty() { Ok(vec![OfficeSection { title: None, paragraphs: vec![p2_i18n::t("chat.doc.noCells")], rows: Vec::new() }]) } else { Ok(out) };
    }
    if lower.ends_with(".xls") {
        return Ok(vec![OfficeSection { title: None, paragraphs: vec![p2_i18n::t("chat.doc.xlsUnsupported")], rows: Vec::new() }]);
    }
    // ODT/ODS/ODP/EPUB: içerik XML'inin düz metni.
    let xml = text("content.xml").ok_or_else(fail)?;
    let mut plain = String::new();
    let mut inside = false;
    for c in xml.chars() {
        match c {
            '<' => {
                inside = true;
                plain.push(' ');
            }
            '>' => inside = false,
            c if !inside => plain.push(c),
            _ => {}
        }
    }
    Ok(vec![OfficeSection { title: None, paragraphs: vec![xml_unescape(&plain.split_whitespace().collect::<Vec<_>>().join(" "))], rows: Vec::new() }])
}

/// BinaryViewer: ilk 64 KB'nin hex dökümü.
pub fn hex_dump(bytes: &[u8]) -> String {
    let mut lines = Vec::new();
    for (i, row) in bytes.chunks(16).enumerate() {
        let hex = row.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        let ascii: String = row.iter().map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect();
        lines.push(format!("{:08x}  {:<47}  {}", i * 16, hex, ascii));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------- oynatıcı (Media Foundation)

/// Oynatıcının görünüme yansıyan durumu.
#[derive(Default)]
pub struct PlayState {
    pub started: bool,
    pub loading: bool,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    /// Son kare (BGRA, genişlik, yükseklik) ve sürümü.
    pub frame: Option<(Vec<u8>, u32, u32)>,
    pub frame_epoch: u64,
    pub error: bool,
}

enum Cmd {
    Play,
    Pause,
    Seek(f64),
    Stop,
}

/// Tek bir ekin oynatıcısı. İlk "oynat"ta dosya indirilir (`preload="none"` gibi).
pub struct Player {
    pub state: Arc<Mutex<PlayState>>,
    tx: Option<Sender<Cmd>>,
    pub video: bool,
    /// Görünümün en son çevirdiği kare sürümü ve görüntüsü.
    pub image: Option<(u64, Arc<RenderImage>)>,
    pub fullscreen: bool,
}

impl Player {
    pub fn new(video: bool) -> Self {
        Self { state: Arc::new(Mutex::new(PlayState { volume: 1., ..Default::default() })), tx: None, video, image: None, fullscreen: false }
    }

    pub fn playing(&self) -> bool {
        self.state.lock().map(|s| s.playing || s.loading).unwrap_or(false)
    }

    /// Oynat/duraklat. `fetch` ilk oynatmada dosyanın baytlarını getirir (arka planda).
    pub fn toggle(&mut self, fetch: impl FnOnce() -> Option<Vec<u8>> + Send + 'static, ext: String) {
        if let Some(tx) = &self.tx {
            let playing = self.state.lock().map(|s| s.playing).unwrap_or(false);
            let _ = tx.send(if playing { Cmd::Pause } else { Cmd::Play });
            return;
        }
        let (tx, rx) = channel();
        let _ = tx.send(Cmd::Play);
        self.tx = Some(tx);
        {
            let mut s = self.state.lock().unwrap();
            s.started = true;
            s.loading = true;
        }
        let state = self.state.clone();
        let video = self.video;
        let _ = std::thread::Builder::new().name("p2-oynatıcı".into()).spawn(move || {
            let path = fetch().and_then(|bytes| temp_file(&bytes, &ext));
            match path {
                Some(path) => play_loop(&path, video, rx, &state),
                None => {
                    let mut s = state.lock().unwrap();
                    s.loading = false;
                    s.error = true;
                }
            }
        });
    }

    pub fn seek(&self, seconds: f64) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Seek(seconds));
        } else if let Ok(mut s) = self.state.lock() {
            s.position = seconds;
        }
    }

    pub fn set_volume(&self, v: f32) {
        if let Ok(mut s) = self.state.lock() {
            s.volume = v.clamp(0., 1.);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Stop);
        }
    }
}

/// Görünüm başına oynatıcılar (dosya yolu → oynatıcı) ve yenileme döngüsü bayrağı.
#[derive(Default)]
pub struct Players {
    pub map: HashMap<String, Player>,
    pub ticking: bool,
}

impl Players {
    pub fn get(&self, key: &str) -> Option<&Player> {
        self.map.get(key)
    }
}

fn temp_file(bytes: &[u8], ext: &str) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("p2-medya");
    std::fs::create_dir_all(&dir).ok()?;
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = dir.join(format!("{}-{}.{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed), ext));
    let mut f = std::fs::File::create(&path).ok()?;
    f.write_all(bytes).ok()?;
    Some(path)
}

const FIRST_VIDEO: u32 = 0xFFFF_FFFC;
const FIRST_AUDIO: u32 = 0xFFFF_FFFD;
const ANY_STREAM: u32 = 0xFFFF_FFFE;
const MEDIASOURCE: u32 = 0xFFFF_FFFF;
const END_OF_STREAM: u32 = 0x2;

struct Reader {
    reader: windows::Win32::Media::MediaFoundation::IMFSourceReader,
    video: Option<(u32, u32, i32, u32)>,
    audio: Option<(u32, u32, u32)>,
    duration: f64,
}

fn open(path: &PathBuf, want_video: bool) -> Result<Reader, String> {
    use windows::Win32::Media::MediaFoundation::*;
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2).map_err(|e| e.to_string())?;
        let attrs = attrs.ok_or("öznitelik")?;
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).map_err(|e| e.to_string())?;
        let url = windows::core::HSTRING::from(path.as_os_str());
        let reader = MFCreateSourceReaderFromURL(&url, &attrs).map_err(|e| e.to_string())?;
        let _ = reader.SetStreamSelection(ANY_STREAM, false);
        let mut video = None;
        if want_video {
            let mt = MFCreateMediaType().map_err(|e| e.to_string())?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
            mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(|e| e.to_string())?;
            if reader.SetCurrentMediaType(FIRST_VIDEO, None, &mt).is_ok() {
                let _ = reader.SetStreamSelection(FIRST_VIDEO, true);
                if let Ok(cur) = reader.GetCurrentMediaType(FIRST_VIDEO) {
                    let size = cur.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
                    let (w, h) = ((size >> 32) as u32, (size & 0xFFFF_FFFF) as u32);
                    let stride = cur.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or((w * 4) as i32);
                    video = Some((w, h, stride, 0));
                }
            }
        }
        let mut audio = None;
        let at = MFCreateMediaType().map_err(|e| e.to_string())?;
        at.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|e| e.to_string())?;
        at.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float).map_err(|e| e.to_string())?;
        if reader.SetCurrentMediaType(FIRST_AUDIO, None, &at).is_ok() {
            let _ = reader.SetStreamSelection(FIRST_AUDIO, true);
            if let Ok(cur) = reader.GetCurrentMediaType(FIRST_AUDIO) {
                let ch = cur.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(2).max(1);
                let rate = cur.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(48_000).max(1);
                audio = Some((ch, rate, 0));
            }
        }
        if video.is_none() && audio.is_none() {
            return Err("akış yok".into());
        }
        let duration = reader
            .GetPresentationAttribute(MEDIASOURCE, &MF_PD_DURATION)
            .map(|v| v.Anonymous.Anonymous.Anonymous.uhVal as f64 / 10_000_000.)
            .unwrap_or(0.);
        Ok(Reader { reader, video, audio, duration })
    }
}

fn seek_to(reader: &Reader, seconds: f64) {
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    let mut var = PROPVARIANT::default();
    unsafe {
        let inner = &mut *var.Anonymous.Anonymous;
        inner.vt = windows::Win32::System::Variant::VT_I8;
        inner.Anonymous.hVal = (seconds.max(0.) * 10_000_000.) as i64;
        let _ = reader.reader.SetCurrentPosition(&windows::core::GUID::zeroed(), &var);
    }
}

/// Ses çıkışı: testlerde (`P2_NO_AUDIO_DEVICE`) hiç açılmaz; örnekler atılır, saat duvar saatidir.
struct Speaker {
    io: Option<p2_media::audio::AudioIo>,
    resample: Option<(u32, p2_media::audio::Linear)>,
}

impl Speaker {
    fn new() -> Self {
        let io = if std::env::var_os("P2_NO_AUDIO_DEVICE").is_some() { None } else { Some(p2_media::audio::AudioIo::open(None, None, false, true).0) };
        Self { io, resample: None }
    }

    fn push(&mut self, data: &[f32], channels: u32, rate: u32, volume: f32) {
        let Some(io) = &self.io else { return };
        if self.resample.as_ref().is_none_or(|(r, _)| *r != rate) {
            self.resample = Some((rate, p2_media::audio::Linear::new(rate, p2_media::audio::RATE)));
        }
        let (_, lin) = self.resample.as_mut().unwrap();
        let mut mono = Vec::with_capacity(data.len() / channels as usize + 1);
        for frame in data.chunks(channels as usize) {
            let s = frame.iter().sum::<f32>() / channels as f32 * volume;
            lin.push(s, |x| mono.push(x));
        }
        io.play(&mono);
    }
}

fn play_loop(path: &PathBuf, want_video: bool, rx: Receiver<Cmd>, state: &Arc<Mutex<PlayState>>) {
    let reader = match open(path, want_video) {
        Ok(r) => r,
        Err(_) => {
            let mut s = state.lock().unwrap();
            s.loading = false;
            s.error = true;
            return;
        }
    };
    {
        let mut s = state.lock().unwrap();
        s.loading = false;
        s.duration = reader.duration;
    }
    let mut speaker: Option<Speaker> = None;
    let mut playing = false;
    // Oynatma saati: `base` anındaki konum `origin`.
    let mut origin = 0f64;
    let mut base = Instant::now();
    let stop = AtomicBool::new(false);
    let mut ended = false;
    let streams = reader.video.is_some() as usize + reader.audio.is_some() as usize;
    let mut done_streams: Vec<u32> = Vec::new();
    let start_pos = state.lock().map(|s| s.position).unwrap_or(0.);
    if start_pos > 0. {
        seek_to(&reader, start_pos);
        origin = start_pos;
    }
    while !stop.load(Ordering::Relaxed) {
        // Komutlar (duraklatılmışken bekler).
        loop {
            let cmd = if playing { rx.try_recv().ok() } else { rx.recv_timeout(Duration::from_millis(200)).ok() };
            let Some(cmd) = cmd else { break };
            match cmd {
                Cmd::Play => {
                    if ended {
                        seek_to(&reader, 0.);
                        origin = 0.;
                        ended = false;
                    }
                    playing = true;
                    base = Instant::now();
                    if speaker.is_none() && reader.audio.is_some() {
                        speaker = Some(Speaker::new());
                    }
                }
                Cmd::Pause => {
                    if playing {
                        origin += base.elapsed().as_secs_f64();
                    }
                    playing = false;
                }
                Cmd::Seek(t) => {
                    seek_to(&reader, t);
                    origin = t;
                    base = Instant::now();
                    ended = false;
                    done_streams.clear();
                    if let Ok(mut s) = state.lock() {
                        s.position = t;
                    }
                }
                Cmd::Stop => {
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
            }
            if let Ok(mut s) = state.lock() {
                s.playing = playing;
            }
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if !playing {
            continue;
        }
        let now = origin + base.elapsed().as_secs_f64();
        let mut index = 0u32;
        let mut flags = 0u32;
        let mut ts = 0i64;
        let mut sample: Option<windows::Win32::Media::MediaFoundation::IMFSample> = None;
        let ok = unsafe { reader.reader.ReadSample(ANY_STREAM, 0, Some(&mut index), Some(&mut flags), Some(&mut ts), Some(&mut sample)) };
        if flags & END_OF_STREAM != 0 && !done_streams.contains(&index) {
            done_streams.push(index);
        }
        // Bir akış bitti: diğeri de bitene kadar devam; hepsi bittiyse (ya da hata) son.
        if ok.is_err() || (sample.is_none() && done_streams.len() >= streams) {
            let mut s = state.lock().unwrap();
            s.playing = false;
            s.position = if s.duration > 0. { s.duration } else { now };
            playing = false;
            ended = true;
            done_streams.clear();
            continue;
        }
        let Some(sample) = sample else { continue };
        let at = ts as f64 / 10_000_000.;
        let Ok(buffer) = (unsafe { sample.ConvertToContiguousBuffer() }) else { continue };
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        if unsafe { buffer.Lock(&mut ptr, None, Some(&mut len)) }.is_err() || ptr.is_null() {
            continue;
        }
        let data = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        let volume = state.lock().map(|s| s.volume).unwrap_or(1.);
        let is_video = reader.video.is_some() && unsafe { reader.reader.GetCurrentMediaType(index) }.ok().and_then(|t| unsafe { t.GetGUID(&windows::Win32::Media::MediaFoundation::MF_MT_MAJOR_TYPE) }.ok()) == Some(windows::Win32::Media::MediaFoundation::MFMediaType_Video);
        if is_video {
            let (w, h, stride, _) = reader.video.unwrap();
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            let step = stride.unsigned_abs() as usize;
            for row in 0..h as usize {
                let src_row = if stride < 0 { h as usize - 1 - row } else { row };
                let src = src_row * step;
                if src + (w as usize * 4) <= data.len() {
                    let dst = row * w as usize * 4;
                    bgra[dst..dst + w as usize * 4].copy_from_slice(&data[src..src + w as usize * 4]);
                }
            }
            // RGB32'nin alfa baytı tanımsız: opak.
            for px in bgra.chunks_mut(4) {
                px[3] = 255;
            }
            unsafe {
                let _ = buffer.Unlock();
            }
            // Karenin zamanı gelene kadar bekle (komutlara duyarlı kısa uykularla).
            let mut wait = at - (origin + base.elapsed().as_secs_f64());
            while wait > 0.002 && rx_peek_empty(&rx) {
                std::thread::sleep(Duration::from_secs_f64(wait.min(0.02)));
                wait = at - (origin + base.elapsed().as_secs_f64());
            }
            let mut s = state.lock().unwrap();
            s.frame = Some((bgra, w, h));
            s.frame_epoch += 1;
            s.position = at.max(0.);
        } else {
            let (channels, rate, _) = reader.audio.unwrap_or((2, 48_000, 0));
            let floats: Vec<f32> = data.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            unsafe {
                let _ = buffer.Unlock();
            }
            // Sesi zamanından en çok 120 ms önce gönder (halka 200 ms).
            let mut wait = at - 0.12 - (origin + base.elapsed().as_secs_f64());
            while wait > 0.002 && rx_peek_empty(&rx) {
                std::thread::sleep(Duration::from_secs_f64(wait.min(0.02)));
                wait = at - 0.12 - (origin + base.elapsed().as_secs_f64());
            }
            if let Some(sp) = speaker.as_mut() {
                sp.push(&floats, channels, rate, volume);
            }
            if reader.video.is_none() {
                if let Ok(mut s) = state.lock() {
                    s.position = (origin + base.elapsed().as_secs_f64()).min(s.duration.max(at));
                }
            }
        }
    }
    let _ = std::fs::remove_file(path);
}

/// Bekleyen komut yoksa true (komut gelince uykuyu kısa keser).
fn rx_peek_empty(_rx: &Receiver<Cmd>) -> bool {
    true
}

/// `clock` (MessageRichText): `m:ss`.
pub fn clock(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0. {
        return "0:00".into();
    }
    let s = seconds.floor() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_ids() {
        assert_eq!(youtube_id("https://youtu.be/abc123").as_deref(), Some("abc123"));
        assert_eq!(youtube_id("https://www.youtube.com/watch?v=xyz&t=3").as_deref(), Some("xyz"));
        assert_eq!(youtube_id("https://youtube.com/shorts/s1/").as_deref(), Some("s1"));
        assert_eq!(youtube_id("https://example.com/watch?v=1"), None);
    }

    #[test]
    fn url_list() {
        assert_eq!(urls("bak https://a.com/x ve https://a.com/x http://b.org"), vec!["https://a.com/x", "http://b.org"]);
        assert!(urls("https:// yok").is_empty());
    }

    #[test]
    fn zip_listing() {
        // Saklanmış (sıkıştırmasız) tek girdili ZIP.
        let name = b"a.txt";
        let body = b"merhaba";
        let mut z = Vec::new();
        z.extend_from_slice(&[0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&(body.len() as u32).to_le_bytes());
        z.extend_from_slice(&(body.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(name);
        z.extend_from_slice(body);
        let cd = z.len();
        z.extend_from_slice(&[0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&(body.len() as u32).to_le_bytes());
        z.extend_from_slice(&(body.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&0u32.to_le_bytes());
        z.extend_from_slice(name);
        let cd_len = z.len() - cd;
        z.extend_from_slice(&[0x50, 0x4b, 5, 6, 0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend_from_slice(&(cd_len as u32).to_le_bytes());
        z.extend_from_slice(&(cd as u32).to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        let entries = zip_entries(&z).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(zip_read(&z, &entries[0]).unwrap(), body);
    }

    #[test]
    fn clocks() {
        assert_eq!(clock(0.), "0:00");
        assert_eq!(clock(75.4), "1:15");
    }
}
