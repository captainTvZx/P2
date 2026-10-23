// D3D12 arka ucu. DXGI Present hook'u d3d11 arka ucuyla ortaktır (swapchain vtable'ı
// d3d11/d3d12 için aynı dxgi.dll nesnesidir); swapchain'in cihazı ID3D12Device ise
// çizim buraya düşer. Komut kuyruğu ID3D12CommandQueue::ExecuteCommandLists hook'uyla
// yakalanır (oyunun DIRECT kuyruğu). Her yeni frameId'de: paylaşılan bellekteki kirli dikdörtgen
// (host'un son ack'ten beri biriktirdiği birleşim; tam kare yalnız dikdörtgen tamsa ya da doku yeniyse)
// başlık mutex'i altında upload tamponuna kopyalanır, CopyTextureRegion ile dokunun o bölgesine aktarılır
// ve tam ekran üçgenle backbuffer üstüne alfa karıştırmalı çizilir. Üç yuvalı halka (allocator + upload + fence) GPU'yu beklemeden
// döner; yalnız aynı yuva hâlâ GPU'daysa beklenir.
// Ekran görüntüsü (Faz 4): istek geldiğinde aynı komut listesine, overlay çiziminden ÖNCE,
// arka tampon -> READBACK tamponu kopyası kaydedilir (PRESENT -> COPY_SOURCE -> RENDER_TARGET/PRESENT);
// yuvanın fence değeri hatırlanır, sonraki Present'lerde fence tamamlanınca Map ile yuvaya okunur.
// Overlay çizilmeyen karede de yalnız kopya içeren bir liste aynı halkadan geçer.
#include "hook_core.h"
#include "ipc_client.h"
#include "input_hooks.h"
#include "shot.h"
#include "log.h"
#include <windows.h>
#include <stdio.h>
#include <d3d12.h>
#include <dxgi1_4.h>
#include <d3dcompiler.h>
#include "MinHook.h"

#pragma comment(lib, "d3d12.lib")
#pragma comment(lib, "dxgi.lib")

bool compileShaderBlobs(ID3DBlob** vs, ID3DBlob** ps); // shaders_d3d11.cpp (sm 5_0)

namespace {
  IpcClient* g_ipc = nullptr;
  typedef void(STDMETHODCALLTYPE* ExecFn)(ID3D12CommandQueue*, UINT, ID3D12CommandList* const*);
  ExecFn g_origExec = nullptr;
  ID3D12CommandQueue* g_queue = nullptr;   // oyunun DIRECT kuyruğu
  ID3D12Device* g_dev = nullptr;
  ID3D12RootSignature* g_rootSig = nullptr;
  ID3D12PipelineState* g_pso = nullptr;
  DXGI_FORMAT g_psoFormat = DXGI_FORMAT_UNKNOWN;
  ID3D12DescriptorHeap* g_rtvHeap = nullptr;
  ID3D12DescriptorHeap* g_srvHeap = nullptr;
  ID3D12GraphicsCommandList* g_list = nullptr;
  ID3D12Fence* g_fence = nullptr; HANDLE g_fenceEvent = nullptr; UINT64 g_fenceValue = 0;
  ID3D12Resource* g_tex = nullptr;
  UINT g_texW = 0, g_texH = 0, g_uploadPitch = 0;
  bool g_texHasFrame = false;
  uint64_t g_lastFrame = 0;
  bool g_failed = false;
  const int kSlots = 3;
  struct Slot { ID3D12CommandAllocator* alloc = nullptr; ID3D12Resource* upload = nullptr; uint8_t* ptr = nullptr; UINT64 fence = 0; };
  Slot g_slots[kSlots];
  int g_slotIndex = 0;

  // Ekran görüntüsü: READBACK tamponu (boyut/format aynıyken yeniden kullanılır) + kopyanın fence değeri.
  ID3D12Resource* g_shotReadback = nullptr;
  UINT g_shotW = 0, g_shotH = 0, g_shotPitch = 0; DXGI_FORMAT g_shotFormat = DXGI_FORMAT_UNKNOWN;
  UINT64 g_shotFence = 0;
  const int kShotMaxPolls = 60; // fence 60 Present'te tamamlanmazsa vazgeç (halka zaten 3 karede bekler)

  void STDMETHODCALLTYPE hookExec(ID3D12CommandQueue* q, UINT n, ID3D12CommandList* const* lists) {
    if (!g_queue && q) { D3D12_COMMAND_QUEUE_DESC d = q->GetDesc(); if (d.Type == D3D12_COMMAND_LIST_TYPE_DIRECT) g_queue = q; }
    g_origExec(q, n, lists);
  }

  void fail(const char* what, HRESULT hr) { if (!g_failed) p2log("d3d12: %s basarisiz (%08x); overlay bu oyunda cizilemiyor", what, hr); g_failed = true; }

  void waitAll() {
    if (!g_queue || !g_fence) return;
    UINT64 v = ++g_fenceValue;
    if (FAILED(g_queue->Signal(g_fence, v))) return;
    if (g_fence->GetCompletedValue() < v) { g_fence->SetEventOnCompletion(v, g_fenceEvent); WaitForSingleObject(g_fenceEvent, 2000); }
  }
  void waitSlot(Slot& s) {
    if (!s.fence || !g_fence) return;
    if (g_fence->GetCompletedValue() < s.fence) { g_fence->SetEventOnCompletion(s.fence, g_fenceEvent); WaitForSingleObject(g_fenceEvent, 2000); }
  }

  template <class T> void release(T*& p) { if (p) { p->Release(); p = nullptr; } }

  void releaseTexture() {
    waitAll();
    release(g_tex);
    for (auto& s : g_slots) { if (s.upload) { s.upload->Unmap(0, nullptr); s.ptr = nullptr; } release(s.upload); }
    g_texW = g_texH = 0; g_texHasFrame = false;
  }
  void releaseShot() {
    if (g_shotReadback) { waitAll(); release(g_shotReadback); } // GPU hâlâ yazıyor olabilir
    g_shotW = g_shotH = g_shotPitch = 0; g_shotFormat = DXGI_FORMAT_UNKNOWN; g_shotFence = 0;
  }
  void releaseAll() {
    releaseTexture();
    releaseShot();
    for (auto& s : g_slots) { release(s.alloc); s.fence = 0; }
    release(g_list); release(g_rtvHeap); release(g_srvHeap); release(g_pso); release(g_rootSig); release(g_fence);
    if (g_fenceEvent) { CloseHandle(g_fenceEvent); g_fenceEvent = nullptr; }
    g_psoFormat = DXGI_FORMAT_UNKNOWN; g_dev = nullptr;
  }

  bool ensureObjects(ID3D12Device* dev) {
    if (g_dev == dev && g_rootSig) return true;
    if (g_dev && g_dev != dev) releaseAll();
    g_dev = dev;
    // Kök imza: t0 tanımlayıcı tablosu (piksel) + statik doğrusal örnekleyici.
    D3D12_DESCRIPTOR_RANGE range{}; range.RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV; range.NumDescriptors = 1; range.BaseShaderRegister = 0; range.OffsetInDescriptorsFromTableStart = 0;
    D3D12_ROOT_PARAMETER param{}; param.ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE; param.DescriptorTable.NumDescriptorRanges = 1; param.DescriptorTable.pDescriptorRanges = &range; param.ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    D3D12_STATIC_SAMPLER_DESC samp{}; samp.Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR; samp.AddressU = samp.AddressV = samp.AddressW = D3D12_TEXTURE_ADDRESS_MODE_CLAMP; samp.MaxLOD = D3D12_FLOAT32_MAX; samp.ShaderRegister = 0; samp.ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    D3D12_ROOT_SIGNATURE_DESC rs{}; rs.NumParameters = 1; rs.pParameters = &param; rs.NumStaticSamplers = 1; rs.pStaticSamplers = &samp; rs.Flags = D3D12_ROOT_SIGNATURE_FLAG_NONE;
    ID3DBlob* sig = nullptr; ID3DBlob* err = nullptr;
    HRESULT hr = D3D12SerializeRootSignature(&rs, D3D_ROOT_SIGNATURE_VERSION_1, &sig, &err);
    if (err) err->Release();
    if (FAILED(hr)) { fail("kok imza serilestirme", hr); return false; }
    hr = dev->CreateRootSignature(0, sig->GetBufferPointer(), sig->GetBufferSize(), IID_PPV_ARGS(&g_rootSig)); sig->Release();
    if (FAILED(hr)) { fail("kok imza", hr); return false; }
    D3D12_DESCRIPTOR_HEAP_DESC hd{}; hd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV; hd.NumDescriptors = 1;
    if (FAILED(hr = dev->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&g_rtvHeap)))) { fail("rtv heap", hr); return false; }
    hd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV; hd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    if (FAILED(hr = dev->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&g_srvHeap)))) { fail("srv heap", hr); return false; }
    for (auto& s : g_slots) if (FAILED(hr = dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&s.alloc)))) { fail("command allocator", hr); return false; }
    if (FAILED(hr = dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, g_slots[0].alloc, nullptr, IID_PPV_ARGS(&g_list)))) { fail("command list", hr); return false; }
    g_list->Close();
    if (FAILED(hr = dev->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&g_fence)))) { fail("fence", hr); return false; }
    g_fenceEvent = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    return true;
  }

  bool ensurePso(DXGI_FORMAT format) {
    if (g_pso && g_psoFormat == format) return true;
    release(g_pso);
    ID3DBlob* vs = nullptr; ID3DBlob* ps = nullptr;
    if (!compileShaderBlobs(&vs, &ps)) { fail("shader derleme", E_FAIL); return false; }
    D3D12_GRAPHICS_PIPELINE_STATE_DESC pd{};
    pd.pRootSignature = g_rootSig;
    pd.VS = { vs->GetBufferPointer(), vs->GetBufferSize() };
    pd.PS = { ps->GetBufferPointer(), ps->GetBufferSize() };
    auto& bt = pd.BlendState.RenderTarget[0];
    // Electron'un offscreen karesi premultiplied BGRA'dır: renk zaten alfayla çarpılmış, kaynak çarpanı ONE olmalı
    // (SRC_ALPHA olsaydı alfa iki kez uygulanır, panel olduğundan çok daha saydam/koyu görünürdü).
    bt.BlendEnable = TRUE; bt.SrcBlend = D3D12_BLEND_ONE; bt.DestBlend = D3D12_BLEND_INV_SRC_ALPHA; bt.BlendOp = D3D12_BLEND_OP_ADD;
    bt.SrcBlendAlpha = D3D12_BLEND_ONE; bt.DestBlendAlpha = D3D12_BLEND_INV_SRC_ALPHA; bt.BlendOpAlpha = D3D12_BLEND_OP_ADD; bt.RenderTargetWriteMask = D3D12_COLOR_WRITE_ENABLE_ALL;
    pd.SampleMask = UINT_MAX;
    pd.RasterizerState.FillMode = D3D12_FILL_MODE_SOLID; pd.RasterizerState.CullMode = D3D12_CULL_MODE_NONE; pd.RasterizerState.DepthClipEnable = TRUE;
    pd.DepthStencilState.DepthEnable = FALSE; pd.DepthStencilState.StencilEnable = FALSE;
    pd.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
    pd.NumRenderTargets = 1; pd.RTVFormats[0] = format; pd.SampleDesc.Count = 1;
    HRESULT hr = g_dev->CreateGraphicsPipelineState(&pd, IID_PPV_ARGS(&g_pso));
    vs->Release(); ps->Release();
    if (FAILED(hr)) { fail("pipeline state", hr); return false; }
    g_psoFormat = format;
    return true;
  }

  bool ensureTexture(UINT w, UINT h) {
    if (g_tex && g_texW == w && g_texH == h) return true;
    releaseTexture();
    D3D12_HEAP_PROPERTIES heap{}; heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    D3D12_RESOURCE_DESC td{}; td.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D; td.Width = w; td.Height = h; td.DepthOrArraySize = 1; td.MipLevels = 1;
    td.Format = DXGI_FORMAT_B8G8R8A8_UNORM; td.SampleDesc.Count = 1; td.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
    HRESULT hr = g_dev->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &td, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, nullptr, IID_PPV_ARGS(&g_tex));
    if (FAILED(hr)) { fail("doku", hr); return false; }
    g_uploadPitch = (w * 4 + D3D12_TEXTURE_DATA_PITCH_ALIGNMENT - 1) & ~(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT - 1);
    D3D12_HEAP_PROPERTIES up{}; up.Type = D3D12_HEAP_TYPE_UPLOAD;
    D3D12_RESOURCE_DESC bd{}; bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER; bd.Width = (UINT64)g_uploadPitch * h; bd.Height = 1; bd.DepthOrArraySize = 1; bd.MipLevels = 1;
    bd.Format = DXGI_FORMAT_UNKNOWN; bd.SampleDesc.Count = 1; bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    for (auto& s : g_slots) {
      if (FAILED(hr = g_dev->CreateCommittedResource(&up, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, IID_PPV_ARGS(&s.upload)))) { fail("upload tamponu", hr); return false; }
      D3D12_RANGE none{ 0, 0 };
      if (FAILED(hr = s.upload->Map(0, &none, (void**)&s.ptr))) { fail("upload map", hr); return false; }
    }
    D3D12_SHADER_RESOURCE_VIEW_DESC sv{}; sv.Format = td.Format; sv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D; sv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING; sv.Texture2D.MipLevels = 1;
    g_dev->CreateShaderResourceView(g_tex, &sv, g_srvHeap->GetCPUDescriptorHandleForHeapStart());
    g_texW = w; g_texH = h; g_lastFrame = 0; g_texHasFrame = false;
    return true;
  }

  // READBACK tamponu: alignedPitch*h bayt; satır adımı 256'ya hizalı (CopyTextureRegion şartı).
  bool ensureReadback(UINT w, UINT h, UINT bpp, DXGI_FORMAT fmt) {
    UINT pitch = (w * bpp + D3D12_TEXTURE_DATA_PITCH_ALIGNMENT - 1) & ~(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT - 1);
    if (g_shotReadback && g_shotW == w && g_shotH == h && g_shotFormat == fmt && g_shotPitch == pitch) return true;
    releaseShot();
    D3D12_HEAP_PROPERTIES hp{}; hp.Type = D3D12_HEAP_TYPE_READBACK;
    D3D12_RESOURCE_DESC bd{}; bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER; bd.Width = (UINT64)pitch * h; bd.Height = 1; bd.DepthOrArraySize = 1; bd.MipLevels = 1;
    bd.Format = DXGI_FORMAT_UNKNOWN; bd.SampleDesc.Count = 1; bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    HRESULT hr = g_dev->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_COPY_DEST, nullptr, IID_PPV_ARGS(&g_shotReadback));
    if (FAILED(hr)) { p2log("d3d12: readback tamponu olusturulamadi (%08x)", hr); return false; }
    g_shotW = w; g_shotH = h; g_shotPitch = pitch; g_shotFormat = fmt;
    return true;
  }

  // Aşama 2: kopyanın fence'i tamamlandıysa READBACK'i eşle, yuvaya sıkı satırlarla yaz, SHOT_READY.
  void shotPoll() {
    if (shot::phase() != shot::COPIED) return;
    uint32_t id = shot::inFlight();
    if (!g_shotReadback || !g_fence) { shot::fail(g_ipc, id, "readback tamponu yok"); return; }
    if (g_fence->GetCompletedValue() < g_shotFence) {
      if (shot::bumpRetry() >= kShotMaxPolls) shot::fail(g_ipc, id, "gpu kopyasi fence ile tamamlanmadi");
      return;
    }
    D3D12_RANGE readRange{ 0, (SIZE_T)g_shotPitch * g_shotH }; void* p = nullptr;
    HRESULT hr = g_shotReadback->Map(0, &readRange, &p);
    if (FAILED(hr) || !p) { char why[64]; _snprintf_s(why, sizeof(why), _TRUNCATE, "readback Map %08x", (unsigned)hr); shot::fail(g_ipc, id, why); return; }
    shot::complete(g_ipc, id, (const uint8_t*)p, g_shotPitch, g_shotW, g_shotH, (uint32_t)g_shotFormat);
    D3D12_RANGE none{ 0, 0 };
    g_shotReadback->Unmap(0, &none);
  }

  D3D12_RESOURCE_BARRIER transition(ID3D12Resource* res, D3D12_RESOURCE_STATES before, D3D12_RESOURCE_STATES after) {
    D3D12_RESOURCE_BARRIER b{}; b.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION; b.Transition.pResource = res; b.Transition.StateBefore = before; b.Transition.StateAfter = after; b.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    return b;
  }
}

namespace d3d12backend {
  // Komut kuyruğu hook'u: geçici cihaz + kuyrukla vtable'daki ExecuteCommandLists adresi bulunur.
  bool install(IpcClient* ipc) {
    g_ipc = ipc;
    ID3D12Device* dev = nullptr;
    HRESULT hr = D3D12CreateDevice(nullptr, D3D_FEATURE_LEVEL_11_0, IID_PPV_ARGS(&dev));
    if (FAILED(hr)) { p2log("d3d12 gecici cihaz olusturulamadi (%08x)", hr); return false; }
    D3D12_COMMAND_QUEUE_DESC qd{}; qd.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    ID3D12CommandQueue* q = nullptr;
    hr = dev->CreateCommandQueue(&qd, IID_PPV_ARGS(&q));
    if (FAILED(hr)) { dev->Release(); p2log("d3d12 gecici kuyruk olusturulamadi (%08x)", hr); return false; }
    void** vtbl = *(void***)q;
    void* exec = vtbl[10]; // ID3D12CommandQueue::ExecuteCommandLists
    q->Release(); dev->Release();
    if (MH_CreateHook(exec, (void*)hookExec, (void**)&g_origExec) != MH_OK) { p2log("MH_CreateHook ExecuteCommandLists basarisiz"); return false; }
    MH_STATUS st = MH_EnableHook(exec);
    if (st != MH_OK && st != MH_ERROR_ENABLED) { p2log("MH_EnableHook ExecuteCommandLists basarisiz"); return false; }
    p2log("d3d12 kuyruk hook kuruldu");
    return true;
  }

  // ResizeBuffers öncesi: backbuffer'a dokunan işler bitsin.
  void onResize() { if (g_dev) waitAll(); }

  // Present'ten çağrılır (swapchain cihazı ID3D12Device ise). drawOverlay: (visible||drawMask) && kare var.
  // Ekran görüntüsü isteği çizimden bağımsız servis edilir. Dönüş: swapchain D3D12 olarak ele alındı mı.
  bool service(IDXGISwapChain* swap, bool drawOverlay) {
    if (!g_ipc) return false;
    P2OverlayHeader* h = g_ipc->header();
    if (!h) return false;
    ID3D12Device* dev = nullptr;
    if (FAILED(swap->GetDevice(IID_PPV_ARGS(&dev))) || !dev) return false;
    dev->Release(); // swapchain yaşadığı sürece cihaz yaşar; referans tutmayız

    // Aşama 2: önceki istek GPU'da bittiyse yuvaya oku (yeni istek işlenmeden önce).
    shotPoll();
    bool shotNew = shot::pending(h);
    uint32_t shotId = shotNew ? h->shotRequestId : 0;
    if (shotNew) shot::begin(shotId);

    bool canDraw = drawOverlay && h->frameId && h->width && h->height;
    if (g_failed) { if (shotNew) shot::fail(g_ipc, shotId, "d3d12 arka ucu devre disi"); return false; }
    if (!g_queue) {
      static bool said = false; if (!said) { p2log("d3d12: komut kuyrugu henuz yakalanmadi"); said = true; }
      if (shotNew) shot::fail(g_ipc, shotId, "komut kuyrugu yok"); // host desktopCapturer'a düşer
      return true;
    }
    if (!ensureObjects(dev)) { if (shotNew) shot::fail(g_ipc, shotId, "d3d12 nesneleri kurulamadi"); return true; }
    if (!shotNew && !canDraw) return true; // bu karede iş yok

    IDXGISwapChain3* swap3 = nullptr; UINT index = 0;
    if (SUCCEEDED(swap->QueryInterface(IID_PPV_ARGS(&swap3))) && swap3) { index = swap3->GetCurrentBackBufferIndex(); swap3->Release(); }
    ID3D12Resource* back = nullptr;
    if (FAILED(swap->GetBuffer(index, IID_PPV_ARGS(&back))) || !back) { if (shotNew) shot::fail(g_ipc, shotId, "GetBuffer"); return true; }
    D3D12_RESOURCE_DESC bd = back->GetDesc();
    input::noteBackbuffer((UINT)bd.Width, bd.Height);
    if (canDraw && (!ensurePso(bd.Format) || !ensureTexture(h->width, h->height))) canDraw = false;
    if (canDraw) {
      static bool saidDraw = false;
      if (!saidDraw) { saidDraw = true; p2log("d3d12: ilk cizim (backbuffer %ux%u fmt=%u, arayuz %ux%u)", (UINT)bd.Width, bd.Height, (UINT)bd.Format, h->width, h->height); }
    }

    // Ekran görüntüsü ön denetimi: format/boyut/readback tamponu (başarısızsa bayrakla, çizime devam).
    bool shotRecord = false;
    if (shotNew) {
      if (bd.SampleDesc.Count > 1) shot::fail(g_ipc, shotId, "msaa arka tampon");
      else if (shot::checkSource(g_ipc, shotId, (uint32_t)bd.Format, (UINT)bd.Width, bd.Height, bd.SampleDesc.Count)) {
        if (ensureReadback((UINT)bd.Width, bd.Height, shot::bytesPerPixel((uint32_t)bd.Format), bd.Format)) shotRecord = true;
        else shot::fail(g_ipc, shotId, "readback tamponu olusturulamadi");
      }
    }
    if (!shotRecord && !canDraw) { back->Release(); return true; }

    Slot& slot = g_slots[g_slotIndex]; g_slotIndex = (g_slotIndex + 1) % kSlots;
    waitSlot(slot);
    slot.alloc->Reset();
    g_list->Reset(slot.alloc, canDraw ? g_pso : nullptr);

    // Ekran görüntüsü kopyası: overlay çiziminden ÖNCE (yuva overlay'siz kare taşır).
    // PRESENT -> COPY_SOURCE -> (çizim varsa RENDER_TARGET, yoksa PRESENT).
    if (shotRecord) {
      D3D12_RESOURCE_BARRIER toCopySrc = transition(back, D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_COPY_SOURCE);
      g_list->ResourceBarrier(1, &toCopySrc);
      D3D12_TEXTURE_COPY_LOCATION dst{}; dst.pResource = g_shotReadback; dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
      dst.PlacedFootprint.Offset = 0; dst.PlacedFootprint.Footprint = { bd.Format, (UINT)bd.Width, bd.Height, 1, g_shotPitch };
      D3D12_TEXTURE_COPY_LOCATION srcLoc{}; srcLoc.pResource = back; srcLoc.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX; srcLoc.SubresourceIndex = 0;
      g_list->CopyTextureRegion(&dst, 0, 0, 0, &srcLoc, nullptr);
      D3D12_RESOURCE_BARRIER after = transition(back, D3D12_RESOURCE_STATE_COPY_SOURCE, canDraw ? D3D12_RESOURCE_STATE_RENDER_TARGET : D3D12_RESOURCE_STATE_PRESENT);
      g_list->ResourceBarrier(1, &after);
    }

    if (canDraw) {
      // Yeni kare geldiyse (ya da doku yeni) kirli dikdörtgeni bu yuvanın upload tamponuna kopyala ve dokuya
      // aktar. Kopya başlık mutex'i altında (host yarım dikdörtgen bırakmaz); kilit alınamazsa bu Present'te
      // eski doku çizilir, g_lastFrame ilerlemez. Dikdörtgen boş/tam ya da doku yeni ise tam kopya.
      // Upload tamponu tam kare için boyutlandırılmıştır; dikdörtgenin hizalı adımı (dw*4 -> 256) ve dh
      // satırı her zaman sığar. Yüklenen frameId mutex altında ack'lenir.
      if (h->frameId != g_lastFrame || !g_texHasFrame) {
        bool locked = g_ipc->lockHeader(8);
        if (locked || !g_ipc->hasLock()) {
          P2FrameView v = g_ipc->frameView();
          if (v.width == g_texW && v.height == g_texH && v.stride >= v.width * 4 && v.pixels) {
            int32_t x = v.dx, y = v.dy, w = v.dw, hh = v.dh;
            bool full = !g_texHasFrame || p2_rect_is_full(x, y, w, hh, v.width, v.height);
            if (full) { x = 0; y = 0; w = (int32_t)v.width; hh = (int32_t)v.height; }
            if (full || p2_rect_clip(&x, &y, &w, &hh, v.width, v.height)) {
              UINT pitch = ((UINT)w * 4 + D3D12_TEXTURE_DATA_PITCH_ALIGNMENT - 1) & ~(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT - 1);
              const uint8_t* src = v.pixels + (size_t)y * v.stride + (size_t)x * 4;
              for (int32_t r = 0; r < hh; r++) memcpy(slot.ptr + (size_t)r * pitch, src + (size_t)r * v.stride, (size_t)w * 4);
              D3D12_RESOURCE_BARRIER toCopy = transition(g_tex, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_COPY_DEST);
              g_list->ResourceBarrier(1, &toCopy);
              D3D12_TEXTURE_COPY_LOCATION dst{}; dst.pResource = g_tex; dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX; dst.SubresourceIndex = 0;
              D3D12_TEXTURE_COPY_LOCATION srcLoc{}; srcLoc.pResource = slot.upload; srcLoc.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
              srcLoc.PlacedFootprint.Offset = 0; srcLoc.PlacedFootprint.Footprint = { DXGI_FORMAT_B8G8R8A8_UNORM, (UINT)w, (UINT)hh, 1, pitch };
              D3D12_BOX box{ 0, 0, 0, (UINT)w, (UINT)hh, 1 };
              g_list->CopyTextureRegion(&dst, (UINT)x, (UINT)y, 0, &srcLoc, &box);
              D3D12_RESOURCE_BARRIER toRead = transition(g_tex, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
              g_list->ResourceBarrier(1, &toRead);
              if (!full) { static bool saidPartial = false; if (!saidPartial) { saidPartial = true; p2log("kismi kare yolu aktif (d3d12 CopyTextureRegion, ilk dikdortgen %dx%d @%d,%d)", w, hh, x, y); } }
            }
            g_texHasFrame = true; g_lastFrame = v.id; g_ipc->ackFrame(v.id);
          }
          if (locked) g_ipc->unlockHeader();
        }
      }
      if (!g_texHasFrame) { // ilk yükleme kilit yüzünden atlandıysa boş doku çizme; liste yine kapanır
        if (shotRecord) { D3D12_RESOURCE_BARRIER back2 = transition(back, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_PRESENT); g_list->ResourceBarrier(1, &back2); }
        canDraw = false;
      }
    }
    if (canDraw) {
      D3D12_CPU_DESCRIPTOR_HANDLE rtv = g_rtvHeap->GetCPUDescriptorHandleForHeapStart();
      g_dev->CreateRenderTargetView(back, nullptr, rtv);
      if (!shotRecord) { // kopya yolunda arka tampon zaten RENDER_TARGET'a geçirildi
        D3D12_RESOURCE_BARRIER toRt = transition(back, D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET);
        g_list->ResourceBarrier(1, &toRt);
      }
      g_list->OMSetRenderTargets(1, &rtv, FALSE, nullptr);
      D3D12_VIEWPORT vp{ 0, 0, (float)bd.Width, (float)bd.Height, 0, 1 }; g_list->RSSetViewports(1, &vp);
      D3D12_RECT sc{ 0, 0, (LONG)bd.Width, (LONG)bd.Height }; g_list->RSSetScissorRects(1, &sc);
      g_list->SetGraphicsRootSignature(g_rootSig);
      ID3D12DescriptorHeap* heaps[] = { g_srvHeap };
      g_list->SetDescriptorHeaps(1, heaps);
      g_list->SetGraphicsRootDescriptorTable(0, g_srvHeap->GetGPUDescriptorHandleForHeapStart());
      g_list->IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
      g_list->DrawInstanced(3, 1, 0, 0);
      D3D12_RESOURCE_BARRIER toPresent = transition(back, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_PRESENT);
      g_list->ResourceBarrier(1, &toPresent);
    }

    HRESULT hr = g_list->Close();
    if (SUCCEEDED(hr)) {
      ID3D12CommandList* lists[] = { g_list };
      g_origExec(g_queue, 1, lists);
      slot.fence = ++g_fenceValue;
      g_queue->Signal(g_fence, slot.fence);
      if (shotRecord) {
        g_shotFence = slot.fence; shot::setPhase(shot::COPIED);
        p2log("shot: istek %u kopyalandi (fmt=%u %ux%u)", shotId, (unsigned)bd.Format, (UINT)bd.Width, bd.Height);
      }
    } else {
      fail("komut listesi kapatma", hr);
      if (shotRecord) shot::fail(g_ipc, shotId, "komut listesi kapatilamadi");
    }
    back->Release();
    return true;
  }

  void uninstall() { releaseAll(); g_queue = nullptr; }
}
