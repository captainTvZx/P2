#pragma once
#include <windows.h>
#include "../shared/p2overlay_ipc.h"

// Kare anlık görüntüsü: başlık mutex'i altında okunur (frameView), kopya bitene kadar mutex tutulur.
struct P2FrameView {
  uint64_t id = 0;
  uint32_t width = 0, height = 0, stride = 0;
  int32_t dx = 0, dy = 0, dw = 0, dh = 0;   // host'un yazdığı birleşik kirli dikdörtgen (W/H 0 = tam)
  const uint8_t* pixels = nullptr;          // yuva 0 (BGRA, tam kare)
};

// DLL tarafı IPC: paylaşılan belleği açar, girdi borusuna yazar.
class IpcClient {
public:
  bool open();                       // mapping + event + pipe'ı açar (host önce kurmuş olmalı)
  void close();
  P2OverlayHeader* header() { return header_; }
  const uint8_t* frontSlot();        // DLL'in okuyacağı kare (BGRA; kısmi kare yolunda hep yuva 0)
  uint8_t* shotSlot();               // ekran görüntüsünü yazacağı yuva
  bool frameChanged(uint64_t& lastId); // frameId değiştiyse true
  // Kısmi kare yolu: başlık alanlarını tek seferde okur (lockHeader altında çağır) ve yüklenen kareyi
  // ack'ler (reserved2[P2_OVL_ACK_FRAME_SLOT] = düşük 32 bit; yine mutex altında, kopyayla birlikte).
  P2FrameView frameView();
  void ackFrame(uint64_t id);
  void sendInput(const P2InputMsg& msg);
  // Başlık mutex'i (host ile ortak): kısayol tablosu gibi çok alanlı okumalar ve kare kopyası için.
  bool lockHeader(DWORD timeoutMs = 50);
  void unlockHeader();
  bool hasLock() const { return lock_ != nullptr; } // mutex açılamadıysa kilitsiz (en iyi çaba) devam edilir
  bool connected() const { return header_ != nullptr; }
private:
  HANDLE mapping_ = nullptr;
  HANDLE frameReady_ = nullptr;
  HANDLE lock_ = nullptr;
  HANDLE pipe_ = INVALID_HANDLE_VALUE;
  uint8_t* base_ = nullptr;
  P2OverlayHeader* header_ = nullptr;
  DWORD pid_ = 0;
};
