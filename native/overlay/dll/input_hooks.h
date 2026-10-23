#pragma once
#include <windows.h>
#include "ipc_client.h"

// Oyun penceresinin girdisini overlay açıkken yakalar ve host'a iletir.
// Overlay kapalıyken girdi oyuna normal akar; yalnız kısayol tablosu her zaman izlenir.
namespace input {
  void install(IpcClient* ipc);
  // Oyun penceresi enjeksiyondan sonra (hatta splash'tan sonra) oluşur; worker bunu
  // düzenli çağırır: pencere bulunana / değişene kadar yeniden dener ve subclass eder.
  void ensureWindow();
  void uninstall();
  // Görünürlüğü uygular (g_visible, ClipCursor, ShowCursor sayacı). Tek doğruluk kaynağı
  // header->visible'dır; worker başlık değişince bunu çağırır (kısayol ya da host kaynaklı).
  void setOverlayVisible(bool visible);
  bool overlayVisible();
  void setHotkey(UINT vk, UINT mods);              // geriye uyum: panel kısayolu (tablo boşsa id 1 olarak kullanılır)
  void setHotkeyTable(const P2Hotkey* table, uint32_t gen); // şema 4 kısayol tablosu (worker, mutex altında kopyalayıp verir)
  uint32_t hotkeyGen();
  void setCursorShape(uint32_t idcId);    // host'un yazdığı IDC_* kimliği (0 = ok); overlay açıkken hemen uygulanır                             // yüklü tablonun nesli (header->hotkeyGen ile karşılaştırılır)
  void noteOutputWindow(HWND hwnd);   // Present hook'u: swapchain cikti penceresi (oyun penceresi adayi)
  void installApiHooks();              // MinHook hazir olduktan sonra: SetCursorPos no-op (overlay acikken)
  void noteBackbuffer(UINT w, UINT h); // cizim arka ucu her Present'te cagirir; degisince host'a RESIZE gider
}
