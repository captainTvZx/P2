// Girdi yakalama: oyun penceresinin WndProc'u subclass edilir. Overlay açıkken
// fare/klavye mesajları overlay'e (host'a boru üzerinden) yönlendirilir ve oyuna
// GEÇMEZ. Raw Input kullanan oyunlar için GetRawInputData da maskelenir.
// Kısayol tablosu (şema 4) overlay açık ya da kapalı her zaman değerlendirilir.
// Bilinen sınır: RIDEV_NOLEGACY ile yalnız ham klavye okuyan oyunlarda WM_KEYDOWN
// hiç gelmez; kısayollar orada çalışmaz (bkz. README).
#include "input_hooks.h"
#include "log.h"
#include <windowsx.h>
#include <string.h>
#include "MinHook.h"

namespace {
  IpcClient* g_ipc = nullptr;
  HWND g_hwnd = nullptr;
  WNDPROC g_orig = nullptr;
  volatile bool g_visible = false;
  UINT g_bbW = 0, g_bbH = 0;   // son bildirilen backbuffer boyutu (overlay yüzeyi buna eşitlenir)
  // UE ve benzeri motorlar fareyi yakalarken her kare SetCursorPos ile imleci ortalar;
  // overlay açıkken bu no-op olur ki overlay imleci yerinde kalsın.
  typedef BOOL(WINAPI* SetCursorPosFn)(int, int);
  SetCursorPosFn g_origSetCursorPos = nullptr;
  BOOL WINAPI hookSetCursorPos(int x, int y) { if (g_visible) return TRUE; return g_origSetCursorPos(x, y); }
  // Oyun her kare SetCursor(NULL)/ShowCursor(FALSE) ile imleci gizler; overlay açıkken bunlar
  // bastırılır, yoksa imleç "pır pır" eder (bir kare görünür, bir kare kaybolur).
  typedef HCURSOR(WINAPI* SetCursorFn)(HCURSOR);
  typedef int(WINAPI* ShowCursorFn)(BOOL);
  SetCursorFn g_origSetCursor = nullptr;
  ShowCursorFn g_origShowCursor = nullptr;
  int g_cursorBumps = 0; // overlay açılırken ShowCursor sayacını kaç kez artırdık (kapanınca geri alınır)
  // Overlay imlecinin şekli: host Chromium'dan (cursor-changed) IDC_* kimliğini başlığa yazar; 32512 = ok.
  volatile uint32_t g_cursorId = 32512;
  HCURSOR overlayCursor() { HCURSOR c = LoadCursorW(nullptr, MAKEINTRESOURCEW(g_cursorId ? g_cursorId : 32512)); return c ? c : LoadCursorW(nullptr, MAKEINTRESOURCEW(32512)); }
  HCURSOR WINAPI hookSetCursor(HCURSOR cursor) { if (g_visible) return g_origSetCursor(overlayCursor()); return g_origSetCursor(cursor); }
  int WINAPI hookShowCursor(BOOL show) { if (g_visible && !show) return 0; return g_origShowCursor(show); }
  void hookApi(HMODULE user32, const char* name, void* detour, void** orig) {
    void* target = (void*)GetProcAddress(user32, name); if (!target) return;
    if (MH_CreateHook(target, detour, orig) != MH_OK) { p2log("girdi: %s hook kurulamadi", name); return; }
    MH_STATUS st = MH_EnableHook(target);
    if (st != MH_OK && st != MH_ERROR_ENABLED) p2log("girdi: %s hook etkinlestirilemedi", name);
  }

  // ---- kısayol tablosu ----
  UINT g_vk = '2';          // geriye uyum: Shift+2 varsayılan panel kısayolu
  UINT g_mods = 0x1;        // bit0 shift
  P2Hotkey g_hotkeys[P2_OVL_MAX_HOTKEYS] = {};
  uint32_t g_hotkeyGen = 0;
  bool g_tableLoaded = false;   // host hiç tablo yazmadıysa hotkeyVk/hotkeyMods'tan id 1 türetilir
  // Tuş basılıyken eşleşen kısayol id'si (vk ile indekslenir): KEYUP'ta modifier bırakılmış olsa da
  // (Shift önce bırakıldı) aynı kısayolun "up" olayı üretilir; bas-konuş mikrofonu açık kalmasın.
  uint8_t g_heldId[256] = {};

  void rebuildFallback() {
    if (g_tableLoaded) return;
    memset(g_hotkeys, 0, sizeof(g_hotkeys));
    g_hotkeys[0].vk = (uint16_t)g_vk; g_hotkeys[0].mods = (uint8_t)(g_mods & 0x7); g_hotkeys[0].id = P2_HOTKEY_PANEL;
  }

  // Anlık modifier kümesi (bit0 shift, bit1 ctrl, bit2 alt). Kısayolun kendisi bir modifier
  // tuşuysa (ör. bas-konuş = Sol Shift) o bit sayılmaz; yoksa kendi kendine eşleşmezdi.
  uint8_t currentMods(UINT vk) {
    uint8_t m = 0;
    if (GetKeyState(VK_SHIFT) & 0x8000) m |= P2_HOTKEY_MOD_SHIFT;
    if (GetKeyState(VK_CONTROL) & 0x8000) m |= P2_HOTKEY_MOD_CTRL;
    if (GetKeyState(VK_MENU) & 0x8000) m |= P2_HOTKEY_MOD_ALT;
    if (vk == VK_SHIFT || vk == VK_LSHIFT || vk == VK_RSHIFT) m &= ~P2_HOTKEY_MOD_SHIFT;
    if (vk == VK_CONTROL || vk == VK_LCONTROL || vk == VK_RCONTROL) m &= ~P2_HOTKEY_MOD_CTRL;
    if (vk == VK_MENU || vk == VK_LMENU || vk == VK_RMENU) m &= ~P2_HOTKEY_MOD_ALT;
    return m;
  }

  // vk + TAM modifier eşleşmesi (Shift+2, Ctrl+Shift+2 ile eşleşmez). Bulunamazsa nullptr.
  const P2Hotkey* matchHotkey(UINT vk) {
    uint8_t mods = currentMods(vk);
    for (int i = 0; i < P2_OVL_MAX_HOTKEYS; i++) {
      const P2Hotkey& hk = g_hotkeys[i];
      if (!hk.vk || !hk.id) continue;
      if (hk.vk == vk && (hk.mods & 0x7) == mods) return &hk;
    }
    return nullptr;
  }
  const P2Hotkey* hotkeyById(uint8_t id) {
    for (int i = 0; i < P2_OVL_MAX_HOTKEYS; i++) if (g_hotkeys[i].id == id && g_hotkeys[i].vk) return &g_hotkeys[i];
    return nullptr;
  }

  void send(uint16_t type, uint16_t flags, int x, int y, int wheel, uint32_t key) {
    if (!g_ipc) return;
    P2InputMsg msg{}; msg.type = type; msg.flags = flags; msg.x = x; msg.y = y; msg.wheelDelta = wheel; msg.key = key;
    g_ipc->sendInput(msg);
  }

  uint16_t mouseFlags() {
    uint16_t f = 0;
    if (GetKeyState(VK_SHIFT) & 0x8000) f |= P2_INPUT_FLAG_SHIFT;
    if (GetKeyState(VK_CONTROL) & 0x8000) f |= P2_INPUT_FLAG_CTRL;
    if (GetKeyState(VK_MENU) & 0x8000) f |= P2_INPUT_FLAG_ALT;
    return f;
  }

  // Panel kısayolu (id 1): yerel toggle. Başlığı hemen burada yaz: host TOGGLE'ı alır almaz okur;
  // worker'ın 16 ms döngüsünü beklerse eski değeri görür ve paneli ters durumda bildirirdi.
  // İmleç/ClipCursor uygulaması worker'da (header->visible değişince setOverlayVisible).
  void togglePanel(HWND hwnd, LPARAM lp) {
    g_visible = !g_visible;
    if (g_ipc && g_ipc->header()) g_ipc->header()->visible = g_visible ? 1 : 0;
    p2log("kisayol: overlay %s (hwnd=%p, tekrar=%u)", g_visible ? "ACIK" : "kapali", hwnd, (unsigned)(lp & 0xffff));
    send(P2_INPUT_TOGGLE, g_visible ? 1 : 0, 0, 0, 0, g_vk);
  }

  // Kısayol tablosu değerlendirmesi. Dönüş: true = mesaj tüketildi (oyuna geçmesin), false = devam.
  // Overlay açık/kapalı fark etmez; g_visible yakalama bloğundan ÖNCE çağrılır.
  bool handleHotkeys(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    UINT vk = (UINT)wp; if (vk >= 256) return false;
    bool down = (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN);
    if (down) {
      if (lp & (1 << 30)) {
        // Otomatik tekrar: basılı tutulan kısayol oyuna/overlay'e sızmasın, ama yeni olay da üretme.
        if (!g_heldId[vk]) return false;
        const P2Hotkey* held = hotkeyById(g_heldId[vk]);
        return !(held && (held->mods & P2_HOTKEY_PASSTHRU));
      }
      const P2Hotkey* hk = matchHotkey(vk);
      if (!hk) return false;
      g_heldId[vk] = hk->id;
      if (hk->id == P2_HOTKEY_PANEL) { togglePanel(hwnd, lp); return true; }
      p2log("hotkey id=%u down (vk=%u mods=%u)", hk->id, hk->vk, hk->mods & 0x7);
      send(P2_INPUT_HOTKEY, P2_INPUT_FLAG_DOWN, (int)hk->vk, (int)(hk->mods & 0x7), 0, hk->id);
      return !(hk->mods & P2_HOTKEY_PASSTHRU);
    }
    // KEYUP: basılıyken eşleşmiş kısayolun bırakılması (modifier'lar artık basılı olmasa da).
    uint8_t id = g_heldId[vk]; if (!id) return false;
    g_heldId[vk] = 0;
    const P2Hotkey* hk = hotkeyById(id);
    bool passthru = hk && (hk->mods & P2_HOTKEY_PASSTHRU);
    if (id == P2_HOTKEY_PANEL) return true;
    if (id == P2_HOTKEY_PTT) {
      p2log("hotkey id=%u up", id);
      send(P2_INPUT_HOTKEY, 0, hk ? (int)hk->vk : (int)vk, hk ? (int)(hk->mods & 0x7) : 0, 0, id);
    }
    return !passthru;
  }

  LRESULT CALLBACK proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    // Kısayollar her zaman izlenir (overlay kapalıyken açmak, bas-konuş, ekran görüntüsü için).
    if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN || msg == WM_KEYUP || msg == WM_SYSKEYUP) {
      if (handleHotkeys(hwnd, msg, wp, lp)) return 0;
    }
    if (g_visible) {
      switch (msg) {
        case WM_MOUSEMOVE: send(P2_INPUT_MOUSE_MOVE, mouseFlags(), GET_X_LPARAM(lp), GET_Y_LPARAM(lp), 0, 0); return 0;
        case WM_LBUTTONDOWN: send(P2_INPUT_MOUSE_BTN, P2_INPUT_FLAG_DOWN | P2_INPUT_FLAG_LEFT | mouseFlags(), GET_X_LPARAM(lp), GET_Y_LPARAM(lp), 0, 1); return 0;
        case WM_LBUTTONUP: send(P2_INPUT_MOUSE_BTN, P2_INPUT_FLAG_LEFT | mouseFlags(), GET_X_LPARAM(lp), GET_Y_LPARAM(lp), 0, 1); return 0;
        case WM_RBUTTONDOWN: send(P2_INPUT_MOUSE_BTN, P2_INPUT_FLAG_DOWN | P2_INPUT_FLAG_RIGHT | mouseFlags(), GET_X_LPARAM(lp), GET_Y_LPARAM(lp), 0, 2); return 0;
        case WM_RBUTTONUP: send(P2_INPUT_MOUSE_BTN, P2_INPUT_FLAG_RIGHT | mouseFlags(), GET_X_LPARAM(lp), GET_Y_LPARAM(lp), 0, 2); return 0;
        case WM_LBUTTONDBLCLK: send(P2_INPUT_MOUSE_BTN, P2_INPUT_FLAG_DOWN | P2_INPUT_FLAG_LEFT | mouseFlags(), GET_X_LPARAM(lp), GET_Y_LPARAM(lp), 0, 1); return 0;
        case WM_MBUTTONDOWN: send(P2_INPUT_MOUSE_BTN, P2_INPUT_FLAG_DOWN | P2_INPUT_FLAG_MIDDLE | mouseFlags(), GET_X_LPARAM(lp), GET_Y_LPARAM(lp), 0, 3); return 0;
        case WM_MBUTTONUP: send(P2_INPUT_MOUSE_BTN, P2_INPUT_FLAG_MIDDLE | mouseFlags(), GET_X_LPARAM(lp), GET_Y_LPARAM(lp), 0, 3); return 0;
        case WM_XBUTTONDOWN: case WM_XBUTTONUP: case WM_RBUTTONDBLCLK: return 0;
        // Tekerlek: lParam EKRAN koordinatı taşır (öteki fare mesajlarının aksine); istemciye çevrilir.
        case WM_MOUSEWHEEL: case WM_MOUSEHWHEEL: {
          POINT pt{ GET_X_LPARAM(lp), GET_Y_LPARAM(lp) }; ScreenToClient(hwnd, &pt);
          uint16_t f = mouseFlags(); if (msg == WM_MOUSEHWHEEL) f |= P2_INPUT_FLAG_HWHEEL;
          send(P2_INPUT_MOUSE_WHEEL, f, pt.x, pt.y, GET_WHEEL_DELTA_WPARAM(wp), 0); return 0;
        }
        // Raw Input (UE yüksek hassasiyetli fare, klavye): oyun görmesin; DefWindowProc tamponu temizler.
        case WM_INPUT: return DefWindowProcW(hwnd, msg, wp, lp);
        case WM_KEYDOWN: case WM_SYSKEYDOWN: {
          uint16_t f = P2_INPUT_FLAG_DOWN | mouseFlags(); if (lp & (1 << 30)) f |= P2_INPUT_FLAG_REPEAT;
          send(P2_INPUT_KEY, f, 0, 0, 0, (uint32_t)wp); return 0;
        }
        case WM_KEYUP: case WM_SYSKEYUP: send(P2_INPUT_KEY, mouseFlags(), 0, 0, 0, (uint32_t)wp); return 0;
        // UTF-16 birimi aynen gider; surrogate çiftlerini host birleştirir.
        case WM_CHAR: send(P2_INPUT_CHAR, 0, 0, 0, 0, (uint32_t)wp); return 0;
        case WM_DEADCHAR: case WM_SYSCHAR: case WM_SYSDEADCHAR: return 0;
        case WM_SETCURSOR: SetCursor(overlayCursor()); return TRUE;
        default: break;
      }
    }
    if (msg == WM_ACTIVATE) {
      bool active = LOWORD(wp) != WA_INACTIVE;
      if (g_ipc && g_ipc->header()) g_ipc->header()->inputFocus = active ? 1 : 0;
      send(P2_INPUT_FOCUS, active ? P2_INPUT_FLAG_DOWN : 0, 0, 0, 0, 0);
      if (!active) memset(g_heldId, 0, sizeof(g_heldId)); // odak gidince KEYUP gelmez; basılı kısayol kaydı sıfırlanır
    }
    return CallWindowProc(g_orig, hwnd, msg, wp, lp);
  }
}

namespace {
  // Sürecin en büyük görünür, sahipsiz üst pencereleri arasından seçer (UE/Unity'de splash
  // penceresi küçüktür; asıl oyun penceresi sonra gelir ve daha büyüktür).
  HWND g_outputHwnd = nullptr; // swapchain'in çıktı penceresi (Present hook'undan); en güvenilir aday
  HWND findGameWindow() {
    if (g_outputHwnd && IsWindow(g_outputHwnd)) return g_outputHwnd;
    HWND fg = GetForegroundWindow(); DWORD pid = 0; if (fg) GetWindowThreadProcessId(fg, &pid);
    if (fg && pid == GetCurrentProcessId() && IsWindowVisible(fg) && !GetWindow(fg, GW_OWNER)) return fg;
    struct Ctx { DWORD pid; HWND hwnd; long area; } ctx{ GetCurrentProcessId(), nullptr, 0 };
    EnumWindows([](HWND h, LPARAM l) -> BOOL {
      auto* c = (Ctx*)l; DWORD wp = 0; GetWindowThreadProcessId(h, &wp);
      if (wp != c->pid || !IsWindowVisible(h) || GetWindow(h, GW_OWNER)) return TRUE;
      RECT r{}; GetWindowRect(h, &r); long area = (long)(r.right - r.left) * (long)(r.bottom - r.top);
      if (area > c->area) { c->area = area; c->hwnd = h; }
      return TRUE; }, (LPARAM)&ctx);
    return ctx.hwnd;
  }
  void hook(HWND hwnd) {
    g_hwnd = hwnd;
    g_orig = (WNDPROC)SetWindowLongPtrW(g_hwnd, GWLP_WNDPROC, (LONG_PTR)proc);
    if (!g_orig) { p2log("girdi: subclass basarisiz (hwnd=%p, err=%lu)", g_hwnd, GetLastError()); g_hwnd = nullptr; return; }
    p2log("girdi hook kuruldu (hwnd=%p)", g_hwnd);
  }
  void unhook() {
    if (g_hwnd && g_orig && IsWindow(g_hwnd)) SetWindowLongPtrW(g_hwnd, GWLP_WNDPROC, (LONG_PTR)g_orig);
    g_hwnd = nullptr; g_orig = nullptr;
  }
  bool g_reportedMissing = false;
}

namespace input {
  void install(IpcClient* ipc) { g_ipc = ipc; rebuildFallback(); ensureWindow(); }
  void ensureWindow() {
    if (!g_ipc) return;
    // Hook'lu pencere yok oldu (splash kapandı) ya da daha büyük/ön plandaki asıl pencere geldi.
    if (g_hwnd && !IsWindow(g_hwnd)) { p2log("girdi: pencere kapandi, yeniden aranacak"); g_hwnd = nullptr; g_orig = nullptr; }
    HWND best = findGameWindow();
    if (!best) { if (!g_reportedMissing) { p2log("girdi: pencere henuz yok, bekleniyor"); g_reportedMissing = true; } return; }
    if (best == g_hwnd) return;
    if (g_hwnd) unhook();
    hook(best);
  }
  void uninstall() { unhook(); g_ipc = nullptr; }
  // Present hook'u swapchain'in penceresini bildirir; bir sonraki ensureWindow o pencereye geçer
  // (splash penceresi yerine asıl oyun penceresi).
  void noteOutputWindow(HWND hwnd) { if (hwnd && hwnd != g_outputHwnd) { g_outputHwnd = hwnd; p2log("girdi: swapchain penceresi %p", hwnd); } }
  // MinHook başlatıldıktan sonra (hooks::install) çağrılır: imleç API hook'ları.
  void installApiHooks() {
    HMODULE user32 = GetModuleHandleW(L"user32.dll"); if (!user32) return;
    hookApi(user32, "SetCursorPos", (void*)hookSetCursorPos, (void**)&g_origSetCursorPos);
    hookApi(user32, "SetCursor", (void*)hookSetCursor, (void**)&g_origSetCursor);
    hookApi(user32, "ShowCursor", (void*)hookShowCursor, (void**)&g_origShowCursor);
  }
  // Backbuffer boyutu değişince host'a bildirilir; overlay yüzeyi aynı boyuta getirilir
  // (böylece fare koordinatları ve çizim 1:1 örtüşür).
  void noteBackbuffer(UINT w, UINT h) {
    if (!w || !h || (w == g_bbW && h == g_bbH)) return;
    g_bbW = w; g_bbH = h;
    send(P2_INPUT_RESIZE, 0, (int)w, (int)h, 0, 0);
  }
  void setOverlayVisible(bool v) {
    g_visible = v;
    if (v) {
      ClipCursor(nullptr);
      // Oyun imleci gizlemişse (sayaç < 0) görünür olana kadar artır; kapanınca aynı sayıda geri al.
      if (g_origShowCursor) { for (int i = 0; i < 16 && g_origShowCursor(TRUE) < 0; i++) g_cursorBumps++; g_cursorBumps++; }
      if (g_origSetCursor) g_origSetCursor(overlayCursor());
    } else {
      if (g_origShowCursor) { while (g_cursorBumps > 0) { g_origShowCursor(FALSE); g_cursorBumps--; } }
    }
  }
  bool overlayVisible() { return g_visible; }
  void setHotkey(UINT vk, UINT mods) { if (vk) { g_vk = vk; g_mods = mods & 0x7; } rebuildFallback(); } // vk 0 = host yazmadi; Shift+2 varsayilani kalir
  void setHotkeyTable(const P2Hotkey* table, uint32_t gen) {
    int n = 0;
    for (int i = 0; i < P2_OVL_MAX_HOTKEYS; i++) if (table[i].vk && table[i].id) n++;
    g_hotkeyGen = gen;
    if (n) {
      memcpy(g_hotkeys, table, sizeof(g_hotkeys)); g_tableLoaded = true;
      // id 1 aynası: TOGGLE mesajı ve log g_vk'yi kullanır.
      const P2Hotkey* panel = hotkeyById(P2_HOTKEY_PANEL); if (panel) { g_vk = panel->vk; g_mods = panel->mods & 0x7; }
    } else {
      // Tablo tamamen boş: geriye uyum, hotkeyVk/hotkeyMods panel kısayolu (id 1) olarak kullanılır.
      g_tableLoaded = false; rebuildFallback(); n = 1;
    }
    memset(g_heldId, 0, sizeof(g_heldId));
    p2log("kisayol tablosu yuklendi (gen=%u, %d adet%s)", gen, n, g_tableLoaded ? "" : ", bos -> hotkeyVk/Mods id 1");
  }
  uint32_t hotkeyGen() { return g_hotkeyGen; }
  void setCursorShape(uint32_t idcId) {
    uint32_t next = idcId ? idcId : 32512;
    if (next == g_cursorId) return;
    g_cursorId = next;
    if (g_visible && g_origSetCursor) g_origSetCursor(overlayCursor());
  }
}
