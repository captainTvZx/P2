//! Yalnız verilen pencerenin (test oyunu) DWM'nin oluşturduğu karesini yakalar:
//! Windows Graphics Capture (GraphicsCaptureItem::CreateForWindow). Masaüstünü ya da
//! başka pencereleri okumaz.
//!
//! Neden PrintWindow değil: Faz 0'da flip-model DXGI takas zincirli pencerede PrintWindow
//! (PW_RENDERFULLCONTENT dahil) oyun içeriğini hiç yakalamadı (enjeksiyonsuz: bembeyaz).

use std::time::{Duration, Instant};

use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::Interface;

/// RGBA8 (alfa 255) + boyut.
pub fn window_rgba(hwnd: HWND) -> Option<(u32, u32, Vec<u8>)> {
    match capture(hwnd) {
        Ok(v) => Some(v),
        Err(e) => {
            println!("[host] pencere yakalanamadı: {e}");
            None
        }
    }
}

fn capture(hwnd: HWND) -> windows::core::Result<(u32, u32, Vec<u8>)> {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            Default::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let (device, context) = (device.unwrap(), context.unwrap());
        let dxgi: IDXGIDevice = device.cast()?;
        let d3d: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;

        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = interop.CreateForWindow(hwnd)?;
        let size = item.Size()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)?;
        let session = pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsBorderRequired(false);
        let _ = session.SetIsCursorCaptureEnabled(false);
        session.StartCapture()?;

        // İlk kare bazen eski boyut/boş olabilir; ikinci kareyi al.
        let started = Instant::now();
        let mut got = 0;
        let frame = loop {
            if let Ok(f) = pool.TryGetNextFrame() {
                got += 1;
                if got >= 2 {
                    break f;
                }
            }
            if started.elapsed() > Duration::from_secs(3) {
                session.Close()?;
                pool.Close()?;
                return Err(windows::core::Error::new(windows::core::HRESULT(0x8000_000Au32 as i32), "kare gelmedi"));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        let texture: ID3D11Texture2D = access.GetInterface()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging: Option<ID3D11Texture2D> = None;
        device.CreateTexture2D(&desc, None, Some(&mut staging))?;
        let staging = staging.unwrap();
        context.CopyResource(&staging, &texture);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let (w, h) = (desc.Width, desc.Height);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            let row = std::slice::from_raw_parts((mapped.pData as *const u8).add(y * mapped.RowPitch as usize), w as usize * 4);
            for (d, s) in rgba[y * w as usize * 4..][..w as usize * 4].chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                d.copy_from_slice(&[s[2], s[1], s[0], 255]);
            }
        }
        context.Unmap(&staging, 0);
        frame.Close()?;
        session.Close()?;
        pool.Close()?;
        Ok((w, h, rgba))
    }
}
