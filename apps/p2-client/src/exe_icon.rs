//! Oyun exe'sinin kendi ikonu (başlatma kartı): exe'deki en büyük ikon (256 px'e kadar) BGRA
//! piksellere çevrilir. Unreal başlatıcısı da oyunun ikonunu taşır.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::RenderImage;

/// `size` px kare ikon; exe'de ikon yoksa ya da okunamazsa `None`.
#[cfg(windows)]
pub fn load(exe: &Path, size: i32) -> Option<Arc<RenderImage>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits, HGDIOBJ};
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO, LR_DEFAULTCOLOR, PrivateExtractIconsW};

    let mut wide: Vec<u16> = exe.as_os_str().encode_wide().collect();
    wide.resize(260, 0);
    let path: &[u16; 260] = wide[..260].try_into().ok()?;
    let mut icon = [HICON::default()];
    let mut id = [0u32];
    // SAFETY: tampon boyutları Win32 sözleşmesine uygun; ikon tanıtıcısı her yolda bırakılır.
    unsafe {
        let n = PrivateExtractIconsW(path, 0, size, size, Some(&mut icon), Some(id.as_mut_ptr()), LR_DEFAULTCOLOR.0);
        if n == 0 || n == u32::MAX || icon[0].is_invalid() {
            return None;
        }
        let mut info = ICONINFO::default();
        let pixels = if GetIconInfo(icon[0], &mut info).is_ok() {
            let dc = CreateCompatibleDC(None);
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: size, biHeight: -size, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
                ..Default::default()
            };
            let mut buf = vec![0u8; (size * size * 4) as usize];
            let lines = GetDIBits(dc, info.hbmColor, 0, size as u32, Some(buf.as_mut_ptr().cast()), &mut bmi, DIB_RGB_COLORS);
            let _ = DeleteDC(dc);
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
            (lines > 0).then_some(buf)
        } else {
            None
        };
        let _ = DestroyIcon(icon[0]);
        let mut buf = pixels?;
        // Eski ikonlarda alfa kanalı boştur: tamamen saydamsa opak say.
        if buf.chunks_exact(4).all(|px| px[3] == 0) {
            for px in buf.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }
        // GDI premultiplied değildir; RenderImage BGRA bekler (zaten BGRA).
        let img = image::RgbaImage::from_raw(size as u32, size as u32, buf)?;
        Some(Arc::new(RenderImage::new(vec![image::Frame::new(img)])))
    }
}

#[cfg(not(windows))]
pub fn load(_exe: &Path, _size: i32) -> Option<Arc<RenderImage>> {
    None
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn explorer_icon() {
        let img = super::load(std::path::Path::new(r"C:\Windows\explorer.exe"), 256).expect("ikon");
        assert_eq!(img.size(0).width.0, 256);
        assert!(super::load(std::path::Path::new(r"C:\yok\yok.exe"), 256).is_none());
    }
}
