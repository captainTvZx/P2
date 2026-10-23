#pragma once
#include <stdint.h>
#include "ipc_client.h"

// Ekran görüntüsü (Faz 4) — arka uçlardan bağımsız ortak durum.
// Host `shotRequestId`'yi artırır; DLL Present hook'unda arka tamponu GPU'da bir ara kaynağa
// kopyalar (overlay çizilmeden ÖNCE), bir ya da birkaç kare sonra CPU'ya okuyup yuvaya yazar,
// başlığı doldurur (`shotW/H/Stride/Format`, `shotFlags`, `shotReadyId`) ve `SHOT_READY` yollar.
// İki aşama GPU'yu bekletmez: D3D11 staging + Map(DO_NOT_WAIT), D3D12 READBACK + fence.
namespace shot {
  enum Phase { IDLE = 0, COPIED = 1 };

  // Yeni istek var mı: shotRequestId != shotReadyId && shotRequestId != g_shotInFlight.
  bool pending(const P2OverlayHeader* h);
  // İsteği üstlen: g_shotInFlight = id, faz IDLE, deneme sayacı sıfır.
  void begin(uint32_t id);
  uint32_t inFlight();
  Phase phase();
  void setPhase(Phase p);
  // GPU kopyası bittikten sonra kaç Present'te okuma denendi (DO_NOT_WAIT yeniden denemeleri).
  int bumpRetry();

  // DXGI formatı için piksel başına bayt; 0 = desteklenmiyor (87/91/28/29/24 -> 4, 10 -> 8).
  uint32_t bytesPerPixel(uint32_t dxgiFormat);
  // Boyut/format ön denetimi: desteklenmiyorsa ya da yuvaya sığmıyorsa fail() çağırıp false döner.
  bool checkSource(IpcClient* ipc, uint32_t id, uint32_t dxgiFormat, uint32_t w, uint32_t h, uint32_t samples);

  // CPU'ya inmiş satırları (srcPitch) yuvaya sıkı (w*bpp) kopyalar, başlığı mutex altında doldurur,
  // SHOT_READY yollar. İstek bu arada değiştiyse (shotRequestId != id) sessizce bırakır.
  void complete(IpcClient* ipc, uint32_t id, const uint8_t* src, uint32_t srcPitch, uint32_t w, uint32_t h, uint32_t dxgiFormat);
  // Başarısız: shotFlags |= 1, shotReadyId = id, SHOT_READY (flags bit0). Host desktopCapturer'a düşer.
  void fail(IpcClient* ipc, uint32_t id, const char* reason);
}
