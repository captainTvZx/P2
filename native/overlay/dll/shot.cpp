// Ekran görüntüsü ortak mantığı (bkz. shot.h). Arka uçlar (backend_d3d11/12.cpp) GPU kopyasını
// yapar; burada istek takibi, yuvaya satır kopyası, başlık yazımı ve SHOT_READY mesajı vardır.
#include "shot.h"
#include "log.h"
#include <string.h>
#include <stdio.h>

namespace {
  uint32_t g_shotInFlight = 0;   // üstlenilen istek id'si (0 = yok)
  shot::Phase g_phase = shot::IDLE;
  int g_retries = 0;

  void sendReady(IpcClient* ipc, uint32_t id, uint32_t w, uint32_t h, uint32_t fmt, bool failed) {
    P2InputMsg m{}; m.type = P2_INPUT_SHOT_READY; m.flags = failed ? 1 : 0;
    m.x = (int32_t)w; m.y = (int32_t)h; m.wheelDelta = (int32_t)fmt; m.key = id;
    ipc->sendInput(m);
  }
}

namespace shot {
  bool pending(const P2OverlayHeader* h) {
    if (!h) return false;
    uint32_t req = h->shotRequestId;
    return req != h->shotReadyId && req != g_shotInFlight;
  }
  void begin(uint32_t id) { g_shotInFlight = id; g_phase = IDLE; g_retries = 0; }
  uint32_t inFlight() { return g_shotInFlight; }
  Phase phase() { return g_phase; }
  void setPhase(Phase p) { g_phase = p; if (p == COPIED) g_retries = 0; }
  int bumpRetry() { return ++g_retries; }

  uint32_t bytesPerPixel(uint32_t f) {
    switch (f) {
      case 87: case 91:           // B8G8R8A8_UNORM(_SRGB)
      case 28: case 29:           // R8G8B8A8_UNORM(_SRGB)
      case 24: return 4;          // R10G10B10A2_UNORM
      case 10: return 8;          // R16G16B16A16_FLOAT
      default: return 0;
    }
  }

  bool checkSource(IpcClient* ipc, uint32_t id, uint32_t fmt, uint32_t w, uint32_t h, uint32_t samples) {
    char why[96];
    uint32_t bpp = bytesPerPixel(fmt);
    if (!bpp) { _snprintf_s(why, sizeof(why), _TRUNCATE, "desteklenmeyen format %u", fmt); fail(ipc, id, why); return false; }
    if (!w || !h) { fail(ipc, id, "arka tampon boyutu sifir"); return false; }
    if ((uint64_t)w * h * bpp > (uint64_t)P2_OVL_SHOT_BYTES || w > P2_OVL_MAX_W || h > P2_OVL_MAX_H) {
      _snprintf_s(why, sizeof(why), _TRUNCATE, "yuvaya sigmiyor (%ux%u fmt=%u)", w, h, fmt); fail(ipc, id, why); return false;
    }
    (void)samples; // MSAA: D3D11 Resolve ile çözer, D3D12 flip-model arka tamponu MSAA olamaz
    return true;
  }

  void complete(IpcClient* ipc, uint32_t id, const uint8_t* src, uint32_t srcPitch, uint32_t w, uint32_t h, uint32_t fmt) {
    P2OverlayHeader* hdr = ipc ? ipc->header() : nullptr;
    uint8_t* slot = ipc ? ipc->shotSlot() : nullptr;
    g_phase = IDLE;
    if (!hdr || !slot || !src) return;
    uint32_t bpp = bytesPerPixel(fmt);
    uint32_t stride = w * bpp;
    if (!bpp || (uint64_t)stride * h > (uint64_t)P2_OVL_SHOT_BYTES) { fail(ipc, id, "satir kopyasi sinir disi"); return; }
    if (hdr->shotRequestId != id) { p2log("shot: istek %u gecersiz (yeni istek %u)", id, hdr->shotRequestId); return; }
    for (uint32_t y = 0; y < h; y++) memcpy(slot + (size_t)y * stride, src + (size_t)y * srcPitch, stride);
    bool locked = ipc->lockHeader(50);
    bool still = hdr->shotRequestId == id;
    if (still) {
      hdr->shotW = w; hdr->shotH = h; hdr->shotStride = stride; hdr->shotFormat = fmt;
      hdr->shotFlags = 0;
      hdr->shotReadyId = id;
    }
    if (locked) ipc->unlockHeader();
    if (!still) { p2log("shot: istek %u gecersiz (yeni istek %u)", id, hdr->shotRequestId); return; }
    sendReady(ipc, id, w, h, fmt, false);
    p2log("shot: hazir %u", id);
  }

  void fail(IpcClient* ipc, uint32_t id, const char* reason) {
    g_phase = IDLE;
    P2OverlayHeader* hdr = ipc ? ipc->header() : nullptr;
    if (!hdr) return;
    bool locked = ipc->lockHeader(50);
    bool still = hdr->shotRequestId == id;
    if (still) { hdr->shotFlags |= 1; hdr->shotReadyId = id; }
    if (locked) ipc->unlockHeader();
    if (still) sendReady(ipc, id, 0, 0, 0, true);
    p2log("shot: basarisiz (%s)", reason ? reason : "?");
  }
}
