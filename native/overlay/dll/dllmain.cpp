// p2overlay64.dll giriş noktası. Enjekte edilince bir worker iş parçacığı başlatır:
// IPC'yi açar, girdi hook'unu ve grafik hook'unu kurar, host'tan gelen görünürlük
// ve kısayol tablosu değişikliklerini uygular. Overlay hatası hiçbir zaman oyunu çökertmemeli.
#include <windows.h>
#include <string.h>
#include "ipc_client.h"
#include "hook_core.h"
#include "input_hooks.h"
#include "log.h"

static IpcClient g_ipc;
static volatile bool g_running = false;
static bool g_lastVisible = false;

// Kısayol tablosunu başlık mutex'i altında kopyalayıp girdi tarafına verir.
static void loadHotkeyTable(P2OverlayHeader* h) {
  P2Hotkey table[P2_OVL_MAX_HOTKEYS];
  bool locked = g_ipc.lockHeader(50);
  uint32_t gen = h->hotkeyGen;
  memcpy(table, h->hotkeys, sizeof(table));
  if (locked) g_ipc.unlockHeader();
  input::setHotkeyTable(table, gen);
}

static DWORD WINAPI worker(LPVOID) {
  char exe[MAX_PATH] = {0}; GetModuleFileNameA(nullptr, exe, MAX_PATH); const char* base = strrchr(exe, '\\'); base = base ? base + 1 : exe;
  p2log("p2overlay64 worker basladi (pid=%lu, exe=%s)", GetCurrentProcessId(), base);
  // Host paylaşılan belleği kurana kadar birkaç saniye dene.
  for (int i = 0; i < 50 && !g_ipc.open(); i++) Sleep(100);
  if (!g_ipc.connected()) { p2log("IPC acilamadi; overlay yok"); return 0; }
  if (g_ipc.header()) {
    input::setHotkey(g_ipc.header()->hotkeyVk, g_ipc.header()->hotkeyMods);
    if (g_ipc.header()->hotkeyGen) loadHotkeyTable(g_ipc.header());
  }
  input::install(&g_ipc);
  if (!hooks::install(&g_ipc)) { p2log("grafik hook kurulamadi"); } else input::installApiHooks();
  g_running = true;
  // Görünürlük: header->visible TEK doğruluk kaynağıdır. Kısayol yolu başlığı anında yazar
  // (TOGGLE ile birlikte); host kapatırken/açarken de yazar. Worker yalnız başlık değişince
  // input::setOverlayVisible ile imleç/ClipCursor durumunu uygular; asla başlığı ezmez.
  // Oyun penceresi (UE: önce splash, sonra asıl pencere) gecikmeli geldiği için
  // pencere araması her 500 ms yinelenir.
  int tick = 0;
  while (g_running) {
    if ((tick++ % 30) == 0) input::ensureWindow();
    P2OverlayHeader* h = g_ipc.header();
    if (h) {
      bool hv = h->visible != 0;
      bool local = input::overlayVisible();
      if (hv != g_lastVisible || hv != local) {
        // Kaynak: kısayol g_visible'ı zaten çevirdiyse yerel; değilse host yazdı (Esc / kapat düğmesi).
        if (hv != local) p2log("gorunurluk: host %s", hv ? "acti" : "kapatti");
        else p2log("gorunurluk: %s", hv ? "ACIK" : "kapali");
        g_lastVisible = hv;
        input::setOverlayVisible(hv);   // kapanışta imleç/ClipCursor aynı tik içinde (<=16 ms) bırakılır
      }
      if (h->hotkeyGen != input::hotkeyGen()) loadHotkeyTable(h);
      input::setCursorShape(h->reserved2[P2_OVL_CURSOR_SLOT]);
    }
    Sleep(16);
  }
  return 0;
}

BOOL APIENTRY DllMain(HMODULE mod, DWORD reason, LPVOID) {
  if (reason == DLL_PROCESS_ATTACH) {
    DisableThreadLibraryCalls(mod);
    CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
  } else if (reason == DLL_PROCESS_DETACH) {
    g_running = false;
    __try { hooks::uninstall(); input::uninstall(); g_ipc.close(); } __except (EXCEPTION_EXECUTE_HANDLER) {}
  }
  return TRUE;
}
