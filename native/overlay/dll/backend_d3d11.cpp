// D3D11 arka ucu: IDXGISwapChain::Present vtable hook'u ile overlay karesini
// (paylaşılan bellekten BGRA) tam ekran bir quad olarak backbuffer üstüne çizer.
// Bu, overlay'in birincil ve en çok test edilmesi gereken yoludur.
// Kısmi kare yolu: doku DEFAULT; her yeni frameId'de başlıktaki kirli dikdörtgen (host'un son ack'ten
// beri biriktirdiği birleşim) D3D11_BOX ile UpdateSubresource edilir, tam kare yalnız dikdörtgen tamsa
// ya da doku yeni oluşturulduysa gider. Kopya başlık mutex'i altında yapılır ve frameId ack'lenir.
// Ekran görüntüsü (Faz 4): her Present'te, çizimden bağımsız olarak, `shotRequestId` değiştiyse
// arka tampon overlay çizilmeden ÖNCE bir staging dokusuna kopyalanır (kare N); sonraki
// Present'lerde (N+1..N+5) Map(DO_NOT_WAIT) ile GPU beklenmeden okunur.
#include "hook_core.h"
#include "ipc_client.h"
#include "input_hooks.h"
#include "shot.h"
#include "log.h"
#include <windows.h>
#include <stdio.h>
#include <d3d11.h>
#include <dxgi.h>
#include "MinHook.h"

#pragma comment(lib, "d3d11.lib")
#pragma comment(lib, "dxgi.lib")

// shaders_d3d11.cpp içinde tanimlı (global bağlantı); anonim ad alanı içinde extern bildirmek
// ic linkage verip link hatasına yol açıyordu.
bool compileShaders(ID3D11Device*, ID3D11VertexShader**, ID3D11PixelShader**);
// backend_d3d12.cpp: swapchain cihazı D3D12 ise çizim/ekran görüntüsü oraya devredilir (Present hook'u ortaktır).
namespace d3d12backend { bool service(IDXGISwapChain*, bool drawOverlay); void onResize(); }

namespace {
  IpcClient* g_ipc = nullptr;
  typedef HRESULT(STDMETHODCALLTYPE* PresentFn)(IDXGISwapChain*, UINT, UINT);
  typedef HRESULT(STDMETHODCALLTYPE* ResizeFn)(IDXGISwapChain*, UINT, UINT, UINT, DXGI_FORMAT, UINT);
  PresentFn g_origPresent = nullptr;
  ResizeFn g_origResize = nullptr;

  ID3D11Device* g_dev = nullptr;
  ID3D11DeviceContext* g_ctx = nullptr;
  ID3D11Texture2D* g_tex = nullptr;      // overlay BGRA dokusu (DEFAULT; UpdateSubresource ile yüklenir)
  bool g_texHasFrame = false;            // doku en az bir kez tam kareyle dolduruldu mu (yeniden oluşturulunca false)
  ID3D11ShaderResourceView* g_srv = nullptr;
  ID3D11RenderTargetView* g_rtv = nullptr;
  ID3D11VertexShader* g_vs = nullptr;
  ID3D11PixelShader* g_ps = nullptr;
  ID3D11BlendState* g_blend = nullptr;
  ID3D11SamplerState* g_sampler = nullptr;
  UINT g_texW = 0, g_texH = 0;
  uint64_t g_lastFrame = 0;

  // Ekran görüntüsü: staging (CPU okur) + MSAA arka tampon için resolve ara dokusu.
  ID3D11Texture2D* g_shotStaging = nullptr;
  ID3D11Texture2D* g_shotResolve = nullptr;
  ID3D11Device* g_shotDev = nullptr;
  D3D11_TEXTURE2D_DESC g_shotDesc{};   // staging'in eşlendiği arka tampon tanımı
  const int kShotMaxRetries = 5;

  // Basit tam ekran üçgen; UV'ler SV_VertexID'den türetilir (vertex buffer yok).
  const char* kVS =
    "struct VSOut{float4 pos:SV_POSITION;float2 uv:TEXCOORD0;};"
    "VSOut main(uint id:SV_VertexID){VSOut o;float2 uv=float2((id<<1)&2,id&2);"
    "o.uv=uv;o.pos=float4(uv.x*2-1,1-uv.y*2,0,1);return o;}";
  const char* kPS =
    "Texture2D t:register(t0);SamplerState s:register(s0);"
    "float4 main(float4 pos:SV_POSITION,float2 uv:TEXCOORD0):SV_TARGET{return t.Sample(s,uv);}";

  void releaseShot() {
    if (g_shotStaging) { g_shotStaging->Release(); g_shotStaging = nullptr; }
    if (g_shotResolve) { g_shotResolve->Release(); g_shotResolve = nullptr; }
    g_shotDev = nullptr; g_shotDesc = {};
  }
  void releaseResources() {
    if (g_srv) { g_srv->Release(); g_srv = nullptr; }
    if (g_tex) { g_tex->Release(); g_tex = nullptr; }
    if (g_rtv) { g_rtv->Release(); g_rtv = nullptr; }
    g_texW = g_texH = 0; g_texHasFrame = false;
  }
  void releaseAll() {
    releaseResources();
    releaseShot();
    if (g_vs) { g_vs->Release(); g_vs = nullptr; }
    if (g_ps) { g_ps->Release(); g_ps = nullptr; }
    if (g_blend) { g_blend->Release(); g_blend = nullptr; }
    if (g_sampler) { g_sampler->Release(); g_sampler = nullptr; }
    if (g_ctx) { g_ctx->Release(); g_ctx = nullptr; }
    g_dev = nullptr;
  }

  // Anlık bağlam: hem çizim hem ekran görüntüsü kullanır (çizim hiç olmasa da gerekir).
  bool ensureContext(ID3D11Device* dev) {
    if (g_ctx && g_dev == dev) return true;
    if (g_ctx) { g_ctx->Release(); g_ctx = nullptr; }
    g_dev = dev; dev->GetImmediateContext(&g_ctx);
    return g_ctx != nullptr;
  }

  bool ensurePipeline(ID3D11Device* dev) {
    if (!ensureContext(dev)) return false;
    if (g_vs) return true;
    // Gömülü HLSL'i derlemek yerine önceden derlenmiş bytecode kullanmak daha
    // güvenli olurdu; basitlik için D3DCompile yerine runtime derleme gerekiyorsa
    // d3dcompiler ile yapılmalı. Burada D3DCompile beklendiği varsayılır.
    if (!compileShaders(dev, &g_vs, &g_ps)) { p2log("d3d11 shader derlenemedi"); return false; }
    D3D11_BLEND_DESC bd{}; bd.RenderTarget[0].BlendEnable = TRUE;
    // Premultiplied kaynak (Electron offscreen): ONE / INV_SRC_ALPHA.
    bd.RenderTarget[0].SrcBlend = D3D11_BLEND_ONE; bd.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_ALPHA; bd.RenderTarget[0].BlendOp = D3D11_BLEND_OP_ADD;
    bd.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_ONE; bd.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_INV_SRC_ALPHA; bd.RenderTarget[0].BlendOpAlpha = D3D11_BLEND_OP_ADD;
    bd.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    dev->CreateBlendState(&bd, &g_blend);
    D3D11_SAMPLER_DESC sd{}; sd.Filter = D3D11_FILTER_MIN_MAG_MIP_LINEAR; sd.AddressU = sd.AddressV = sd.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
    dev->CreateSamplerState(&sd, &g_sampler);
    return true;
  }

  // Overlay dokusu DEFAULT (CPU erişimi yok): kare UpdateSubresource ile yüklenir; kısmi kare yolunda
  // yalnız kirli dikdörtgen (D3D11_BOX) gider. Boyut değişince yeniden oluşturulur (g_texHasFrame = false).
  bool ensureTexture(ID3D11Device* dev, UINT w, UINT h) {
    if (g_tex && g_texW == w && g_texH == h) return true;
    releaseResources();
    D3D11_TEXTURE2D_DESC td{}; td.Width = w; td.Height = h; td.MipLevels = 1; td.ArraySize = 1;
    td.Format = DXGI_FORMAT_B8G8R8A8_UNORM; td.SampleDesc.Count = 1; td.Usage = D3D11_USAGE_DEFAULT;
    td.BindFlags = D3D11_BIND_SHADER_RESOURCE; td.CPUAccessFlags = 0;
    if (FAILED(dev->CreateTexture2D(&td, nullptr, &g_tex))) return false;
    if (FAILED(dev->CreateShaderResourceView(g_tex, nullptr, &g_srv))) return false;
    g_texW = w; g_texH = h; g_texHasFrame = false; return true;
  }

  // Yeni kare varsa (frameId != g_lastFrame) paylaşılan yuvadan dokuya yükler. Başlık mutex'i kopya boyunca
  // tutulur (host yarım dikdörtgen bırakmaz); kilit alınamazsa bu Present atlanır, sonraki denemede yeniden
  // okunur (g_lastFrame ilerlemez). Tam yükleme: dikdörtgen boş/tam ya da doku yeni oluşturulmuşsa.
  // Yüklenen frameId mutex altında ack'lenir (host bir sonraki dikdörtgeni sıfırdan başlatır).
  void uploadFrame() {
    P2OverlayHeader* h = g_ipc->header();
    if (h->frameId == g_lastFrame && g_texHasFrame) return;
    bool locked = g_ipc->lockHeader(8);
    if (!locked && g_ipc->hasLock()) return; // host yazıyor; sonraki Present'te
    P2FrameView v = g_ipc->frameView();
    if (v.width == g_texW && v.height == g_texH && v.stride >= v.width * 4 && v.pixels) {
      int32_t x = v.dx, y = v.dy, w = v.dw, hh = v.dh;
      bool full = !g_texHasFrame || p2_rect_is_full(x, y, w, hh, v.width, v.height);
      if (full) {
        g_ctx->UpdateSubresource(g_tex, 0, nullptr, v.pixels, v.stride, 0);
      } else if (p2_rect_clip(&x, &y, &w, &hh, v.width, v.height)) {
        D3D11_BOX box{ (UINT)x, (UINT)y, 0, (UINT)(x + w), (UINT)(y + hh), 1 };
        g_ctx->UpdateSubresource(g_tex, 0, &box, v.pixels + (size_t)y * v.stride + (size_t)x * 4, v.stride, 0);
        static bool saidPartial = false;
        if (!saidPartial) { saidPartial = true; p2log("kismi kare yolu aktif (d3d11 UpdateSubresource box, ilk dikdortgen %dx%d @%d,%d)", w, hh, x, y); }
      }
      g_texHasFrame = true; g_lastFrame = v.id; g_ipc->ackFrame(v.id);
    }
    if (locked) g_ipc->unlockHeader();
  }

  // ---- Ekran görüntüsü (D3D11) ----
  // Staging dokusu arka tamponla aynı format/boyutta (CopyResource şartı), tek örnekli.
  // Arka tampon MSAA ise önce DEFAULT bir dokuya ResolveSubresource, sonra CopyResource.
  bool ensureShotTextures(ID3D11Device* dev, const D3D11_TEXTURE2D_DESC& back) {
    bool same = g_shotStaging && g_shotDev == dev && g_shotDesc.Width == back.Width && g_shotDesc.Height == back.Height
      && g_shotDesc.Format == back.Format && g_shotDesc.SampleDesc.Count == back.SampleDesc.Count;
    if (same) return true;
    releaseShot();
    D3D11_TEXTURE2D_DESC td{}; td.Width = back.Width; td.Height = back.Height; td.MipLevels = 1; td.ArraySize = 1;
    td.Format = back.Format; td.SampleDesc.Count = 1; td.SampleDesc.Quality = 0;
    td.Usage = D3D11_USAGE_STAGING; td.BindFlags = 0; td.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    if (FAILED(dev->CreateTexture2D(&td, nullptr, &g_shotStaging))) { releaseShot(); return false; }
    if (back.SampleDesc.Count > 1) {
      D3D11_TEXTURE2D_DESC rd = td; rd.Usage = D3D11_USAGE_DEFAULT; rd.CPUAccessFlags = 0;
      if (FAILED(dev->CreateTexture2D(&rd, nullptr, &g_shotResolve))) { releaseShot(); return false; }
    }
    g_shotDev = dev; g_shotDesc = back;
    return true;
  }

  // Aşama 1 (kare N): yeni istek -> arka tamponu staging'e kopyala (overlay çizilmeden önce).
  // Aşama 2 (kare N+1..N+5): Map(DO_NOT_WAIT); GPU bitirmediyse sonraki kareye bırak.
  void shotTick(ID3D11Device* dev, IDXGISwapChain* swap) {
    P2OverlayHeader* h = g_ipc->header();
    if (!h) return;
    if (shot::pending(h)) {
      uint32_t id = h->shotRequestId;
      shot::begin(id);
      if (!ensureContext(dev)) { shot::fail(g_ipc, id, "d3d11 baglam yok"); return; }
      ID3D11Texture2D* back = nullptr;
      if (FAILED(swap->GetBuffer(0, __uuidof(ID3D11Texture2D), (void**)&back)) || !back) { shot::fail(g_ipc, id, "GetBuffer"); return; }
      D3D11_TEXTURE2D_DESC bd{}; back->GetDesc(&bd);
      if (!shot::checkSource(g_ipc, id, (uint32_t)bd.Format, bd.Width, bd.Height, bd.SampleDesc.Count)) { back->Release(); return; }
      if (!ensureShotTextures(dev, bd)) { back->Release(); shot::fail(g_ipc, id, "staging doku olusturulamadi"); return; }
      if (bd.SampleDesc.Count > 1) {
        g_ctx->ResolveSubresource(g_shotResolve, 0, back, 0, bd.Format);
        g_ctx->CopyResource(g_shotStaging, g_shotResolve);
      } else {
        g_ctx->CopyResource(g_shotStaging, back);
      }
      back->Release();
      shot::setPhase(shot::COPIED);
      p2log("shot: istek %u kopyalandi (fmt=%u %ux%u)", id, (unsigned)bd.Format, bd.Width, bd.Height);
      return; // okuma en erken bir sonraki Present'te
    }
    if (shot::phase() != shot::COPIED) return;
    uint32_t id = shot::inFlight();
    if (!g_shotStaging || !g_ctx) { shot::fail(g_ipc, id, "staging doku yok"); return; }
    D3D11_MAPPED_SUBRESOURCE map{};
    HRESULT hr = g_ctx->Map(g_shotStaging, 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT, &map);
    if (hr == DXGI_ERROR_WAS_STILL_DRAWING) {
      if (shot::bumpRetry() >= kShotMaxRetries) shot::fail(g_ipc, id, "gpu kopyasi 5 karede bitmedi");
      return;
    }
    if (FAILED(hr)) { char why[64]; _snprintf_s(why, sizeof(why), _TRUNCATE, "Map %08x", (unsigned)hr); shot::fail(g_ipc, id, why); return; }
    shot::complete(g_ipc, id, (const uint8_t*)map.pData, map.RowPitch, g_shotDesc.Width, g_shotDesc.Height, (uint32_t)g_shotDesc.Format);
    g_ctx->Unmap(g_shotStaging, 0);
  }

  void drawOverlay(ID3D11Device* dev, IDXGISwapChain* swap) {
    P2OverlayHeader* h = g_ipc->header();
    if (!h) return;
    if (!ensurePipeline(dev)) return;
    if (!ensureTexture(dev, h->width, h->height)) return;
    uploadFrame();              // frameId değiştiyse (ya da doku yeni) kirli dikdörtgeni/tam kareyi yükle
    if (!g_texHasFrame) return; // ilk yükleme kilit yüzünden atlandıysa boş doku çizme
    // Backbuffer RTV
    ID3D11Texture2D* back = nullptr;
    if (FAILED(swap->GetBuffer(0, __uuidof(ID3D11Texture2D), (void**)&back)) || !back) return;
    if (g_rtv) { g_rtv->Release(); g_rtv = nullptr; }
    dev->CreateRenderTargetView(back, nullptr, &g_rtv);
    back->Release();
    if (!g_rtv) return;
    // Durum kaydet/çiz/geri yükle (oyunun pipeline'ını bozmamak için kaba koruma)
    g_ctx->OMSetRenderTargets(1, &g_rtv, nullptr);
    DXGI_SWAP_CHAIN_DESC scd{}; swap->GetDesc(&scd);
    input::noteBackbuffer(scd.BufferDesc.Width, scd.BufferDesc.Height);
    D3D11_VIEWPORT vp{ 0, 0, (float)scd.BufferDesc.Width, (float)scd.BufferDesc.Height, 0, 1 };
    g_ctx->RSSetViewports(1, &vp);
    float blend[4] = { 0, 0, 0, 0 };
    g_ctx->OMSetBlendState(g_blend, blend, 0xffffffff);
    g_ctx->VSSetShader(g_vs, nullptr, 0);
    g_ctx->PSSetShader(g_ps, nullptr, 0);
    g_ctx->PSSetShaderResources(0, 1, &g_srv);
    g_ctx->PSSetSamplers(0, 1, &g_sampler);
    g_ctx->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    g_ctx->IASetInputLayout(nullptr);
    g_ctx->Draw(3, 0);
  }

  // Her Present'te: önce ekran görüntüsü isteği (çizimden bağımsız, overlay'siz kare), sonra
  // (visible || drawMask) iken overlay çizimi. Swapchain cihazı D3D12 ise ikisi de oraya devredilir.
  void service(IDXGISwapChain* swap) {
    P2OverlayHeader* h = g_ipc->header();
    if (!h) return;
    static bool saidFrame = false, saidShow = false;
    if (!saidFrame && h->frameId) { saidFrame = true; p2log("ilk arayuz karesi alindi (%ux%u)", h->width, h->height); }
    bool draw = (h->visible || h->drawMask) && h->width && h->height && h->frameId;
    if (draw && !saidShow) { saidShow = true; p2log("cizim basliyor (visible=%u drawMask=%u frame=%llu)", h->visible, h->drawMask, (unsigned long long)h->frameId); }
    ID3D11Device* dev = nullptr;
    if (FAILED(swap->GetDevice(__uuidof(ID3D11Device), (void**)&dev)) || !dev) { d3d12backend::service(swap, draw); return; }
    shotTick(dev, swap);
    if (draw) drawOverlay(dev, swap);
    dev->Release();
  }

  // Kare sayacı yalnız swapchain'in çıktı penceresi GÖRÜNÜRKEN artar: UE açılışta splash
  // görselinin arkasında, henüz gösterilmeyen pencereye kare basar; "hazır" sinyali ve
  // başlangıç bildirimi oyuncunun gerçekten gördüğü ilk kareyle gelsin.
  IDXGISwapChain* g_lastSwap = nullptr; HWND g_outputHwnd = nullptr;
  bool outputVisible(IDXGISwapChain* swap) {
    if (swap != g_lastSwap) {
      DXGI_SWAP_CHAIN_DESC d{}; g_outputHwnd = SUCCEEDED(swap->GetDesc(&d)) ? d.OutputWindow : nullptr;
      g_lastSwap = swap; input::noteOutputWindow(g_outputHwnd);
    }
    return g_outputHwnd && IsWindowVisible(g_outputHwnd) && !IsIconic(g_outputHwnd);
  }
  HRESULT STDMETHODCALLTYPE hookPresent(IDXGISwapChain* swap, UINT sync, UINT flags) {
    static int said = 0;
    if (said < 3) { said++; p2log("present #%d (ipc=%d header=%d)", said, g_ipc ? 1 : 0, (g_ipc && g_ipc->header()) ? 1 : 0); }
    __try { if (g_ipc && g_ipc->header()) { bool vis = outputVisible(swap); if (said == 3 && vis) { said++; p2log("present: pencere gorunur (hwnd=%p)", g_outputHwnd); } if (vis) g_ipc->header()->frames++; service(swap); } }
    __except (EXCEPTION_EXECUTE_HANDLER) { p2log("d3d11 present hook istisnasi; cizim atlandi"); }
    return g_origPresent(swap, sync, flags);
  }
  HRESULT STDMETHODCALLTYPE hookResize(IDXGISwapChain* swap, UINT count, UINT w, UINT h, DXGI_FORMAT fmt, UINT flags) {
    if (g_rtv) { g_rtv->Release(); g_rtv = nullptr; } // backbuffer referansları bırakılmalı
    d3d12backend::onResize();
    HRESULT hr = g_origResize(swap, count, w, h, fmt, flags);
    if (w && h) { P2InputMsg m{}; m.type = P2_INPUT_RESIZE; m.x = w; m.y = h; g_ipc->sendInput(m); }
    return hr;
  }
}

namespace d3d11backend {
  // Geçici cihaz + swapchain ile gerçek Present/Resize adreslerini çözüp hook kurar.
  bool install(IpcClient* ipc) {
    g_ipc = ipc;
    WNDCLASSEXW wc{ sizeof(wc) }; wc.lpfnWndProc = DefWindowProcW; wc.hInstance = GetModuleHandleW(nullptr); wc.lpszClassName = L"P2OvlDummy";
    RegisterClassExW(&wc);
    HWND hwnd = CreateWindowW(wc.lpszClassName, L"", WS_OVERLAPPEDWINDOW, 0, 0, 8, 8, nullptr, nullptr, wc.hInstance, nullptr);
    DXGI_SWAP_CHAIN_DESC scd{}; scd.BufferCount = 1; scd.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    scd.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT; scd.OutputWindow = hwnd; scd.SampleDesc.Count = 1; scd.Windowed = TRUE;
    scd.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;
    IDXGISwapChain* swap = nullptr; ID3D11Device* dev = nullptr; ID3D11DeviceContext* ctx = nullptr;
    D3D_FEATURE_LEVEL fl;
    HRESULT hr = D3D11CreateDeviceAndSwapChain(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, nullptr, 0, D3D11_SDK_VERSION, &scd, &swap, &dev, &fl, &ctx);
    if (FAILED(hr)) { if (hwnd) DestroyWindow(hwnd); UnregisterClassW(wc.lpszClassName, wc.hInstance); p2log("d3d11 gecici cihaz olusturulamadi (%08x)", hr); return false; }
    void** vtbl = *(void***)swap;
    void* present = vtbl[8];   // IDXGISwapChain::Present
    void* resize = vtbl[13];   // IDXGISwapChain::ResizeBuffers
    swap->Release(); dev->Release(); ctx->Release();
    DestroyWindow(hwnd); UnregisterClassW(wc.lpszClassName, wc.hInstance);
    if (MH_CreateHook(present, (void*)hookPresent, (void**)&g_origPresent) != MH_OK) { p2log("MH_CreateHook present basarisiz"); return false; }
    MH_CreateHook(resize, (void*)hookResize, (void**)&g_origResize);
    if (MH_EnableHook(MH_ALL_HOOKS) != MH_OK) { p2log("MH_EnableHook basarisiz"); return false; }
    p2log("d3d11 hook kuruldu");
    return true;
  }
  void uninstall() { MH_DisableHook(MH_ALL_HOOKS); releaseAll(); }
}
