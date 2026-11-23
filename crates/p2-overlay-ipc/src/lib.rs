//! Oyun içi overlay IPC — `native/overlay/shared/p2overlay_ipc.h` (şema 4) Rust karşılığı.
//!
//! Host tarafı: paylaşımlı belleği, kare-hazır olayını, başlık kilidini ve girdi borusunu
//! oluşturur; enjekte edilen C++ DLL bunlara bağlanır. Yapı düzeni C++ başlığıyla birebir
//! aynıdır — MSVC ile `offsetof` çıktısı alınarak doğrulandı (aşağıdaki derleme anı
//! denetimleri). Mantık `native/overlay/host/host.cc` ile aynıdır (kısmi kare + ack birleşimi).

#![allow(clippy::missing_safety_doc)]
// Paylaşımlı bellek/boru oturumu yalnız Windows'ta (oyun içi overlay); diğer sistemlerde yalnız `proto`.
#![cfg_attr(not(windows), allow(unused_imports, dead_code))]

pub mod proto;

use std::mem::{offset_of, size_of};
use std::ptr::{addr_of, addr_of_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile, FILE_FLAGS_AND_ATTRIBUTES};
#[cfg(windows)]
use windows::Win32::System::Memory::{CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile};
#[cfg(windows)]
use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE, PIPE_WAIT};
#[cfg(windows)]
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, ReleaseMutex, SetEvent, WaitForSingleObject};
#[cfg(windows)]
use windows::core::{HSTRING, PCWSTR};

pub const MAGIC: u32 = 0x5032_4F56; // 'P2OV'
pub const SCHEMA: u32 = 4;
pub const MAX_W: u32 = 3840;
pub const MAX_H: u32 = 2160;
pub const SLOT_BYTES: usize = (MAX_W * MAX_H * 4) as usize;
pub const SHOT_BYTES: usize = SLOT_BYTES * 2;
pub const ACK_FRAME_SLOT: usize = 0;
pub const CURSOR_SLOT: usize = 1;

pub const DRAW_TOAST: u32 = 0x1;
pub const DRAW_PINNED: u32 = 0x2;
pub const DRAW_HUD: u32 = 0x4;

pub const HOTKEY_PANEL: u8 = 1;
pub const MAX_HOTKEYS: usize = 8;

#[repr(C, packed(4))]
#[derive(Clone, Copy, Default, Debug)]
pub struct Hotkey {
    pub vk: u16,
    pub mods: u8,
    pub id: u8,
}

#[repr(C, packed(4))]
pub struct Header {
    pub magic: u32,
    pub schema: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
    pub front_index: u32,
    pub visible: u32,
    pub frame_id: u64,
    pub dirty_x: i32,
    pub dirty_y: i32,
    pub dirty_w: i32,
    pub dirty_h: i32,
    pub shot_request_id: u32,
    pub shot_ready_id: u32,
    pub hotkey_vk: u32,
    pub hotkey_mods: u32,
    pub draw_mask: u32,
    pub frames: u32,
    pub reserved: [u32; 6],
    pub hotkey_gen: u32,
    pub hotkeys: [Hotkey; MAX_HOTKEYS],
    pub shot_w: u32,
    pub shot_h: u32,
    pub shot_stride: u32,
    pub shot_format: u32,
    pub shot_flags: u32,
    pub input_focus: u32,
    pub reserved2: [u32; 8],
}

#[repr(C, packed(1))]
#[derive(Clone, Copy, Default, Debug)]
pub struct InputMsg {
    pub kind: u16,
    pub flags: u16,
    pub x: i32,
    pub y: i32,
    pub wheel_delta: i32,
    pub key: u32,
    pub reserved: u32,
}

pub mod input {
    pub const MOUSE_MOVE: u16 = 1;
    pub const MOUSE_BTN: u16 = 2;
    pub const MOUSE_WHEEL: u16 = 3;
    pub const KEY: u16 = 4;
    pub const CHAR: u16 = 5;
    pub const TOGGLE: u16 = 6;
    pub const FOCUS: u16 = 7;
    pub const RESIZE: u16 = 8;
    pub const HOTKEY: u16 = 9;
    pub const SHOT_READY: u16 = 10;
    pub const FLAG_DOWN: u16 = 0x0001;
    pub const FLAG_LEFT: u16 = 0x0020;
}

// MSVC `offsetof` çıktısıyla aynı (native/overlay/shared/p2overlay_ipc.h, şema 4).
const _: () = {
    assert!(size_of::<Header>() == 196);
    assert!(size_of::<InputMsg>() == 24);
    assert!(size_of::<Hotkey>() == 4);
    assert!(offset_of!(Header, visible) == 28);
    assert!(offset_of!(Header, frame_id) == 32);
    assert!(offset_of!(Header, dirty_x) == 40);
    assert!(offset_of!(Header, shot_request_id) == 56);
    assert!(offset_of!(Header, draw_mask) == 72);
    assert!(offset_of!(Header, frames) == 76);
    assert!(offset_of!(Header, reserved) == 80);
    assert!(offset_of!(Header, hotkey_gen) == 104);
    assert!(offset_of!(Header, hotkeys) == 108);
    assert!(offset_of!(Header, shot_w) == 140);
    assert!(offset_of!(Header, shot_flags) == 156);
    assert!(offset_of!(Header, input_focus) == 160);
    assert!(offset_of!(Header, reserved2) == 164);
};

pub const fn frame_slot_offset(index: usize) -> usize {
    size_of::<Header>() + index * SLOT_BYTES
}
pub const fn shot_slot_offset() -> usize {
    size_of::<Header>() + 2 * SLOT_BYTES
}
pub const fn total_mapping_bytes() -> usize {
    size_of::<Header>() + 2 * SLOT_BYTES + SHOT_BYTES
}
const _: () = assert!(total_mapping_bytes() == 132_710_596);

/// Kirli dikdörtgen kırpma (`p2_rect_clip`).
pub fn rect_clip(x: &mut i32, y: &mut i32, w: &mut i32, h: &mut i32, fw: u32, fh: u32) -> bool {
    let (x0, y0) = ((*x).max(0) as i64, (*y).max(0) as i64);
    let (x1, y1) = ((*x as i64 + *w as i64).min(fw as i64), (*y as i64 + *h as i64).min(fh as i64));
    if x1 <= x0 || y1 <= y0 {
        *w = 0;
        *h = 0;
        return false;
    }
    (*x, *y, *w, *h) = (x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32);
    true
}

/// `p2_rect_is_full`.
pub fn rect_is_full(x: i32, y: i32, w: i32, h: i32, fw: u32, fh: u32) -> bool {
    w <= 0 || h <= 0 || (x <= 0 && y <= 0 && x as i64 + w as i64 >= fw as i64 && y as i64 + h as i64 >= fh as i64)
}

#[derive(Debug, Clone, Copy)]
pub struct Stats {
    pub frames: u32,
    pub visible: bool,
    pub draw_mask: u32,
    pub frame_id: u64,
    pub ack_frame_id: u32,
    pub input_focus: bool,
}

pub struct Shot {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    /// Sıkı BGRA8, alfa 255.
    pub bgra: Vec<u8>,
}

#[cfg(windows)]
/// Bir oyun süreci için overlay oturumu (host.cc `Session`).
pub struct Session {
    pub pid: u32,
    mapping: HANDLE,
    base: *mut u8,
    frame_ready: HANDLE,
    lock: HANDLE,
    running: Arc<AtomicBool>,
    pipe_thread: Option<JoinHandle<()>>,
    pub input: Receiver<InputMsg>,
}

#[cfg(windows)]
unsafe impl Send for Session {}

#[cfg(windows)]
fn wide(s: &str) -> HSTRING {
    HSTRING::from(s)
}

#[cfg(windows)]
impl Session {
    pub fn create(pid: u32, hotkeys: &[Hotkey]) -> windows::core::Result<Self> {
        unsafe {
            let bytes = total_mapping_bytes() as u64;
            let mapping = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                (bytes >> 32) as u32,
                bytes as u32,
                &wide(&format!("Local\\P2Overlay_Frame_{pid}")),
            )?;
            let view = MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, total_mapping_bytes());
            if view.Value.is_null() {
                let _ = CloseHandle(mapping);
                return Err(windows::core::Error::from_thread());
            }
            let base = view.Value as *mut u8;
            std::ptr::write_bytes(base, 0, size_of::<Header>());
            let h = base as *mut Header;
            addr_of_mut!((*h).magic).write_unaligned(MAGIC);
            addr_of_mut!((*h).schema).write_unaligned(SCHEMA);
            let mut table = [Hotkey::default(); MAX_HOTKEYS];
            for (slot, hk) in table.iter_mut().zip(hotkeys) {
                *slot = *hk;
                if hk.id == HOTKEY_PANEL {
                    addr_of_mut!((*h).hotkey_vk).write_unaligned(hk.vk as u32);
                    addr_of_mut!((*h).hotkey_mods).write_unaligned((hk.mods & 0x7) as u32);
                }
            }
            addr_of_mut!((*h).hotkeys).write_unaligned(table);
            addr_of_mut!((*h).hotkey_gen).write_unaligned(1);

            let frame_ready = CreateEventW(None, false, false, &wide(&format!("Local\\P2Overlay_FrameReady_{pid}")))?;
            let lock = CreateMutexW(None, false, &wide(&format!("Local\\P2Overlay_Lock_{pid}")))?;

            let running = Arc::new(AtomicBool::new(true));
            let (tx, rx) = channel();
            let r = running.clone();
            let pipe_thread = std::thread::Builder::new().name(format!("p2-overlay-pipe-{pid}")).spawn(move || pipe_loop(pid, r, tx)).ok();
            Ok(Self { pid, mapping, base, frame_ready, lock, running, pipe_thread, input: rx })
        }
    }

    fn header(&self) -> *mut Header {
        self.base as *mut Header
    }

    fn lock_header(&self) -> bool {
        let r = unsafe { WaitForSingleObject(self.lock, 50) };
        r.0 == 0 || r.0 == 0x80 // WAIT_OBJECT_0 / WAIT_ABANDONED
    }
    fn unlock_header(&self, locked: bool) {
        if locked {
            unsafe {
                let _ = ReleaseMutex(self.lock);
            }
        }
    }

    /// Önceden çarpılmış BGRA tam kareyi (`w*h*4`) yazar. `dirty` verilirse yalnız o bölge
    /// kopyalanır (yuvada aynı boyutta tam kare yoksa tam kopyaya düşer). host.cc `WriteFrame`.
    pub fn write_frame(&self, bgra: &[u8], w: u32, h: u32, dirty: Option<(i32, i32, i32, i32)>) -> bool {
        if w == 0 || h == 0 || w > MAX_W || h > MAX_H || bgra.len() < (w * h * 4) as usize {
            return false;
        }
        unsafe {
            let hd = self.header();
            let stride = (w * 4) as usize;
            let slot = self.base.add(frame_slot_offset(0));
            let has_frame = addr_of!((*hd).frame_id).read_unaligned() != 0
                && addr_of!((*hd).width).read_unaligned() == w
                && addr_of!((*hd).height).read_unaligned() == h;
            let mut rect = dirty.filter(|_| has_frame);
            if let Some((x, y, rw, rh)) = rect.as_mut() {
                if !rect_clip(x, y, rw, rh, w, h) {
                    return false;
                }
            }
            let locked = self.lock_header();
            match rect {
                None => std::ptr::copy_nonoverlapping(bgra.as_ptr(), slot, (w * h * 4) as usize),
                Some((x, y, rw, rh)) => {
                    for row in y..y + rh {
                        let off = row as usize * stride + x as usize * 4;
                        std::ptr::copy_nonoverlapping(bgra.as_ptr().add(off), slot.add(off), rw as usize * 4);
                    }
                }
            }
            let frame_id = addr_of!((*hd).frame_id).read_unaligned();
            let ack = addr_of!((*hd).reserved2).read_unaligned()[ACK_FRAME_SLOT];
            let consumed = ack == frame_id as u32;
            let (px, py, pw, ph) = (
                addr_of!((*hd).dirty_x).read_unaligned(),
                addr_of!((*hd).dirty_y).read_unaligned(),
                addr_of!((*hd).dirty_w).read_unaligned(),
                addr_of!((*hd).dirty_h).read_unaligned(),
            );
            let (nx, ny, nw, nh) = match rect {
                None => (0, 0, w as i32, h as i32),
                Some(r) if consumed => r,
                Some(_) if rect_is_full(px, py, pw, ph, w, h) => (0, 0, w as i32, h as i32),
                Some((cx, cy, cw, ch)) => {
                    let (x0, y0) = (px.min(cx), py.min(cy));
                    let (x1, y1) = ((px + pw).max(cx + cw), (py + ph).max(cy + ch));
                    (x0, y0, x1 - x0, y1 - y0)
                }
            };
            addr_of_mut!((*hd).width).write_unaligned(w);
            addr_of_mut!((*hd).height).write_unaligned(h);
            addr_of_mut!((*hd).stride).write_unaligned(w * 4);
            addr_of_mut!((*hd).dirty_x).write_unaligned(nx);
            addr_of_mut!((*hd).dirty_y).write_unaligned(ny);
            addr_of_mut!((*hd).dirty_w).write_unaligned(nw);
            addr_of_mut!((*hd).dirty_h).write_unaligned(nh);
            addr_of_mut!((*hd).front_index).write_unaligned(0);
            addr_of_mut!((*hd).frame_id).write_unaligned(frame_id + 1);
            self.unlock_header(locked);
            let _ = SetEvent(self.frame_ready);
        }
        true
    }

    /// Kısayol tablosunu canlı günceller (DLL `hotkey_gen` değişince yeniden okur).
    pub fn set_hotkeys(&self, hotkeys: &[Hotkey]) {
        let locked = self.lock_header();
        unsafe {
            let h = self.header();
            let mut table = [Hotkey::default(); MAX_HOTKEYS];
            for (slot, hk) in table.iter_mut().zip(hotkeys) {
                *slot = *hk;
                if hk.id == HOTKEY_PANEL {
                    addr_of_mut!((*h).hotkey_vk).write_unaligned(hk.vk as u32);
                    addr_of_mut!((*h).hotkey_mods).write_unaligned((hk.mods & 0x7) as u32);
                }
            }
            addr_of_mut!((*h).hotkeys).write_unaligned(table);
            let generation = addr_of!((*h).hotkey_gen).read_unaligned().wrapping_add(1);
            addr_of_mut!((*h).hotkey_gen).write_unaligned(generation);
        }
        self.unlock_header(locked);
    }

    /// Overlay açıkken DLL'in kullanacağı imleç (Win32 IDC_* kimliği; 0 = ok).
    pub fn set_cursor(&self, idc: u32) {
        unsafe {
            let h = self.header();
            let mut r = addr_of!((*h).reserved2).read_unaligned();
            r[CURSOR_SLOT] = idc;
            addr_of_mut!((*h).reserved2).write_unaligned(r);
        }
    }

    pub fn set_visible(&self, on: bool) {
        unsafe { addr_of_mut!((*self.header()).visible).write_volatile(on as u32) }
    }
    pub fn set_draw_mask(&self, mask: u32) {
        unsafe { addr_of_mut!((*self.header()).draw_mask).write_volatile(mask & 0xF) }
    }

    pub fn stats(&self) -> Stats {
        unsafe {
            let h = self.header();
            Stats {
                frames: addr_of!((*h).frames).read_unaligned(),
                visible: addr_of!((*h).visible).read_unaligned() != 0,
                draw_mask: addr_of!((*h).draw_mask).read_unaligned(),
                frame_id: addr_of!((*h).frame_id).read_unaligned(),
                ack_frame_id: addr_of!((*h).reserved2).read_unaligned()[ACK_FRAME_SLOT],
                input_focus: addr_of!((*h).input_focus).read_unaligned() != 0,
            }
        }
    }

    /// DLL'den oyunun arka tamponunu iste (overlay çizilmeden önceki kare).
    pub fn request_shot(&self) -> u32 {
        let locked = self.lock_header();
        let id = unsafe {
            let h = self.header();
            addr_of_mut!((*h).shot_flags).write_unaligned(0);
            let id = addr_of!((*h).shot_request_id).read_unaligned() + 1;
            addr_of_mut!((*h).shot_request_id).write_unaligned(id);
            id
        };
        self.unlock_header(locked);
        id
    }

    /// Hazırsa ekran görüntüsünü sıkı BGRA8 olarak döndürür (BGRA8 / RGBA8 kaynaklar).
    pub fn read_shot(&self) -> Option<Shot> {
        let locked = self.lock_header();
        let (req, ready, flags, w, h, stride, fmt) = unsafe {
            let hd = self.header();
            (
                addr_of!((*hd).shot_request_id).read_unaligned(),
                addr_of!((*hd).shot_ready_id).read_unaligned(),
                addr_of!((*hd).shot_flags).read_unaligned(),
                addr_of!((*hd).shot_w).read_unaligned(),
                addr_of!((*hd).shot_h).read_unaligned(),
                addr_of!((*hd).shot_stride).read_unaligned(),
                addr_of!((*hd).shot_format).read_unaligned(),
            )
        };
        self.unlock_header(locked);
        if req == 0 || ready != req || flags & 1 != 0 || w == 0 || h == 0 || w > MAX_W || h > MAX_H {
            return None;
        }
        // Kaynak biçimi: 8 bit BGRA/RGBA, 10 bit R10G10B10A2 (Unreal D3D12 varsayılanı) ve 16 bit
        // kayan noktalı HDR (scRGB, doğrusal → sRGB). Önceden yalnız 8 bit okunuyor, Unreal
        // oyunlarında ekran görüntüsü "alınamadı" diyordu.
        let bpp: u32 = match fmt {
            87 | 91 | 28 | 29 | 24 => 4,
            10 => 8,
            _ => return None,
        };
        if stride < w * bpp || stride as usize * h as usize > SHOT_BYTES {
            return None;
        }
        let src = unsafe { std::slice::from_raw_parts(self.base.add(shot_slot_offset()), stride as usize * h as usize) };
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            let row = &src[y * stride as usize..][..(w * bpp) as usize];
            let dst = &mut out[y * w as usize * 4..][..w as usize * 4];
            match fmt {
                87 | 91 => dst.copy_from_slice(row),
                28 | 29 => {
                    for (d, s) in dst.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                        d.copy_from_slice(&[s[2], s[1], s[0], 255]);
                    }
                }
                24 => {
                    for (d, s) in dst.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                        let p = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
                        let (r, g, b) = ((p & 0x3ff) >> 2, ((p >> 10) & 0x3ff) >> 2, ((p >> 20) & 0x3ff) >> 2);
                        d.copy_from_slice(&[b as u8, g as u8, r as u8, 255]);
                    }
                }
                _ => {
                    for (d, s) in dst.chunks_exact_mut(4).zip(row.chunks_exact(8)) {
                        let c = |i: usize| linear_to_srgb8(f16_to_f32(u16::from_le_bytes([s[i], s[i + 1]])));
                        d.copy_from_slice(&[c(4), c(2), c(0), 255]);
                    }
                }
            }
            for d in dst.chunks_exact_mut(4) {
                d[3] = 255;
            }
        }
        Some(Shot { id: ready, width: w, height: h, bgra: out })
    }
}

/// IEEE 754 yarım duyarlıklı sayı → f32.
#[cfg(windows)]
fn f16_to_f32(h: u16) -> f32 {
    let (sign, exp, man) = ((h >> 15) as u32, ((h >> 10) & 0x1f) as u32, (h & 0x3ff) as u32);
    let bits = match (exp, man) {
        (0, 0) => sign << 31,
        (0, m) => {
            // Alt normal: normalleştir.
            let mut e = 127 - 15 + 1;
            let mut m = m;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((m & 0x3ff) << 13)
        }
        (0x1f, m) => (sign << 31) | (0xff << 23) | (m << 13),
        (e, m) => (sign << 31) | ((e + 127 - 15) << 23) | (m << 13),
    };
    f32::from_bits(bits)
}

/// Doğrusal [0,1] → sRGB 8 bit (HDR değerleri kırpılır).
#[cfg(windows)]
fn linear_to_srgb8(v: f32) -> u8 {
    let v = if v.is_nan() { 0. } else { v.clamp(0., 1.) };
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1. / 2.4) - 0.055 };
    (s * 255. + 0.5) as u8
}

#[cfg(windows)]
impl Drop for Session {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        // Bekleyen ConnectNamedPipe'ı uyandırmak için boruya kendimiz bağlanırız.
        unsafe {
            if let Ok(h) = CreateFileW(
                &wide(&format!("\\\\.\\pipe\\P2Overlay_Input_{}", self.pid)),
                GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            ) {
                let _ = CloseHandle(h);
            }
        }
        // DLL boruya bağlıysa iş parçacığı ReadFile'da bloke: okumayı iptal edip bekle.
        if let Some(t) = self.pipe_thread.take() {
            use std::os::windows::io::AsRawHandle;
            unsafe {
                let _ = windows::Win32::System::IO::CancelSynchronousIo(HANDLE(t.as_raw_handle()));
            }
            let _ = t.join();
        }
        unsafe {
            let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.base as _ });
            let _ = CloseHandle(self.mapping);
            let _ = CloseHandle(self.frame_ready);
            let _ = CloseHandle(self.lock);
        }
    }
}

/// host.cc `pipeLoop`: DLL boruya yazar (sabit 24 baytlık kayıt), biz okuyup kanala aktarırız.
#[cfg(windows)]
fn pipe_loop(pid: u32, running: Arc<AtomicBool>, tx: Sender<InputMsg>) {
    let name = wide(&format!("\\\\.\\pipe\\P2Overlay_Input_{pid}"));
    while running.load(Ordering::SeqCst) {
        let pipe = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                1,
                0,
                64 * 1024,
                0,
                None,
            )
        };
        if pipe.is_invalid() {
            std::thread::sleep(std::time::Duration::from_millis(200));
            continue;
        }
        let connected = unsafe { ConnectNamedPipe(pipe, None) }
            .map(|_| true)
            .unwrap_or_else(|e| e.code() == ERROR_PIPE_CONNECTED.to_hresult());
        if connected {
            let mut msg = InputMsg::default();
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                let mut read = 0u32;
                let buf = unsafe { std::slice::from_raw_parts_mut(&mut msg as *mut InputMsg as *mut u8, size_of::<InputMsg>()) };
                let ok = unsafe { ReadFile(pipe, Some(buf), Some(&mut read), None) }.is_ok();
                if !ok || read as usize != size_of::<InputMsg>() {
                    break;
                }
                if tx.send(msg).is_err() {
                    break;
                }
            }
        }
        unsafe {
            let _ = DisconnectNamedPipe(pipe);
            let _ = CloseHandle(pipe);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn rect_helpers_match_c() {
        let (mut x, mut y, mut w, mut h) = (-10, 5, 30, 2000);
        assert!(rect_clip(&mut x, &mut y, &mut w, &mut h, 100, 100));
        assert_eq!((x, y, w, h), (0, 5, 20, 95));
        assert!(rect_is_full(0, 0, 0, 0, 10, 10));
        assert!(rect_is_full(-1, -1, 20, 20, 10, 10));
        assert!(!rect_is_full(1, 0, 10, 10, 10, 10));
    }

    #[test]
    fn session_roundtrip_in_process() {
        // Kendi pid'imizle oturum kur, kare yaz, başlığı oku (DLL olmadan).
        let s = Session::create(std::process::id(), &[Hotkey { vk: 0x77, mods: 0, id: HOTKEY_PANEL }]).unwrap();
        let frame = vec![0x80u8; 64 * 32 * 4];
        assert!(s.write_frame(&frame, 64, 32, None));
        assert!(s.write_frame(&frame, 64, 32, Some((10, 10, 5, 5))));
        let st = s.stats();
        assert_eq!(st.frame_id, 2);
        unsafe {
            let h = s.header();
            // Ack yok → ikinci dikdörtgen tam kareyle birleşir (tam kalır).
            assert_eq!(addr_of!((*h).dirty_w).read_unaligned(), 64);
            assert_eq!(addr_of!((*h).hotkey_vk).read_unaligned(), 0x77);
        }
    }
}
