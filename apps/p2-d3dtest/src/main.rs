//! Faz 0 / Spike 3 — overlay testi için küçük bir "oyun" penceresi.
//!
//!   p2-d3dtest d3d11 | d3d12
//!
//! Renk değiştiren arka plan + kayan bir şerit çizer, flip-model takas zinciriyle
//! vsync'te sunar ve FPS'i başlığa yazar. Gerçek oyun gibi: overlay DLL'i bu sürecin
//! Present çağrısını yakalar ve kendi katmanını arka tampona bindirir.

use std::mem::ManuallyDrop;
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, Interface, PCWSTR, w};

const W: i32 = 1280;
const H: i32 = 720;
const BUFFERS: u32 = 2;

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_DESTROY {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

fn create_window(title: &str) -> anyhow::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = w!("P2D3DTest");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let style = WS_OVERLAPPEDWINDOW & !WS_THICKFRAME & !WS_MAXIMIZEBOX;
        let mut rect = RECT { left: 0, top: 0, right: W, bottom: H };
        AdjustWindowRect(&mut rect, style, false)?;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            &HSTRING::from(title),
            style | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        Ok(hwnd)
    }
}

/// Mesajları işler; pencere kapandıysa false.
fn pump() -> bool {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

/// Zamana göre arka plan rengi ve şerit konumu.
fn scene(t: f32) -> ([f32; 4], [f32; 4], RECT) {
    let bg = [0.10 + 0.08 * (t * 0.7).sin(), 0.16 + 0.06 * (t * 0.5).cos(), 0.28 + 0.10 * (t * 0.9).sin(), 1.0];
    let bar = [0.95, 0.62, 0.18, 1.0];
    let x = ((t * 240.0) as i32) % (W + 200) - 200;
    (bg, bar, RECT { left: x, top: H / 2 - 40, right: x + 200, bottom: H / 2 + 40 })
}

struct Fps {
    hwnd: HWND,
    api: &'static str,
    last: Instant,
    frames: u32,
}
impl Fps {
    fn tick(&mut self) {
        self.frames += 1;
        let dt = self.last.elapsed().as_secs_f32();
        if dt >= 1.0 {
            let title = format!("P2 {} test — {:.0} FPS", self.api, self.frames as f32 / dt);
            unsafe {
                let _ = SetWindowTextW(self.hwnd, &HSTRING::from(title));
            }
            self.frames = 0;
            self.last = Instant::now();
        }
    }
}

fn swap_desc() -> DXGI_SWAP_CHAIN_DESC1 {
    DXGI_SWAP_CHAIN_DESC1 {
        Width: W as u32,
        Height: H as u32,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: BUFFERS,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        ..Default::default()
    }
}

fn run_d3d11(hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            Default::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let (device, context) = (device.unwrap(), context.unwrap());
        let context1: ID3D11DeviceContext1 = context.cast()?;
        let factory: IDXGIFactory2 = CreateDXGIFactory1()?;
        let swap = factory.CreateSwapChainForHwnd(&device, hwnd, &swap_desc(), None, None)?;
        let back: ID3D11Texture2D = swap.GetBuffer(0)?;
        let mut rtv: Option<ID3D11RenderTargetView> = None;
        device.CreateRenderTargetView(&back, None, Some(&mut rtv))?;
        let rtv = rtv.unwrap();

        let start = Instant::now();
        let mut fps = Fps { hwnd, api: "D3D11", last: Instant::now(), frames: 0 };
        while pump() {
            let (bg, bar, rect) = scene(start.elapsed().as_secs_f32());
            context.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
            context.ClearRenderTargetView(&rtv, &bg);
            context1.ClearView(&rtv, &bar, Some(&[rect]));
            let _ = swap.Present(1, DXGI_PRESENT(0));
            fps.tick();
        }
    }
    Ok(())
}

fn transition(resource: &ID3D12Resource, before: D3D12_RESOURCE_STATES, after: D3D12_RESOURCE_STATES) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: ManuallyDrop::new(Some(resource.clone())),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    }
}

fn release_barrier(b: D3D12_RESOURCE_BARRIER) {
    unsafe {
        let t = ManuallyDrop::into_inner(b.Anonymous.Transition);
        drop(ManuallyDrop::into_inner(t.pResource));
    }
}

fn run_d3d12(hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        let mut device: Option<ID3D12Device> = None;
        D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device)?;
        let device = device.unwrap();
        let queue: ID3D12CommandQueue =
            device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC { Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default() })?;
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
        let swap: IDXGISwapChain3 = factory.CreateSwapChainForHwnd(&queue, hwnd, &swap_desc(), None, None)?.cast()?;

        let heap: ID3D12DescriptorHeap = device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            NumDescriptors: BUFFERS,
            ..Default::default()
        })?;
        let step = device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) as usize;
        let first = heap.GetCPUDescriptorHandleForHeapStart();
        let mut buffers = Vec::new();
        for i in 0..BUFFERS {
            let buf: ID3D12Resource = swap.GetBuffer(i)?;
            device.CreateRenderTargetView(&buf, None, D3D12_CPU_DESCRIPTOR_HANDLE { ptr: first.ptr + i as usize * step });
            buffers.push(buf);
        }
        let allocator: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
        let list: ID3D12GraphicsCommandList = device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?;
        list.Close()?;
        let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
        let event = CreateEventW(None, false, false, PCWSTR::null())?;
        let mut fence_value = 0u64;

        let start = Instant::now();
        let mut fps = Fps { hwnd, api: "D3D12", last: Instant::now(), frames: 0 };
        while pump() {
            let idx = swap.GetCurrentBackBufferIndex() as usize;
            let rtv = D3D12_CPU_DESCRIPTOR_HANDLE { ptr: first.ptr + idx * step };
            let (bg, bar, rect) = scene(start.elapsed().as_secs_f32());
            allocator.Reset()?;
            list.Reset(&allocator, None)?;
            let to_rt = transition(&buffers[idx], D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET);
            list.ResourceBarrier(std::slice::from_ref(&to_rt));
            list.ClearRenderTargetView(rtv, &bg, None);
            list.ClearRenderTargetView(rtv, &bar, Some(&[rect]));
            let to_present = transition(&buffers[idx], D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_PRESENT);
            list.ResourceBarrier(std::slice::from_ref(&to_present));
            list.Close()?;
            queue.ExecuteCommandLists(&[Some(list.cast()?)]);
            release_barrier(to_rt);
            release_barrier(to_present);
            let _ = swap.Present(1, DXGI_PRESENT(0));
            fence_value += 1;
            queue.Signal(&fence, fence_value)?;
            if fence.GetCompletedValue() < fence_value {
                fence.SetEventOnCompletion(fence_value, event)?;
                WaitForSingleObject(event, INFINITE);
            }
            fps.tick();
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let api = std::env::args().nth(1).unwrap_or_else(|| "d3d11".into()).to_lowercase();
    let hwnd = create_window(&format!("P2 {} test", api.to_uppercase()))?;
    match api.as_str() {
        "d3d12" => run_d3d12(hwnd),
        _ => run_d3d11(hwnd),
    }
}
