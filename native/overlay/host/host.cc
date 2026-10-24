// p2overlayhost.node — Electron ana süreci tarafında çalışan N-API eklentisi.
// Her oyun pid'i için paylaşılan belleği ve adlandırılmış boru sunucusunu kurar.
// Electron overlay karelerini writeFrame ile yazar; DLL'den gelen girdi mesajlarını
// bir iş parçacığında okuyup threadsafe callback ile JS'e iletir.
// Şema 4: kısayol tablosu (setHotkeys), çizim maskesi (setDrawMask), ekran görüntüsü
// isteği/okuması (requestShot/readShot), stats ve windowTitle.
// Kısmi kare yolu (partialFrames=true): tek kalıcı yuva, kirli dikdörtgen yazımı, ack ile birleşim.
#include <napi.h>
#include <windows.h>
#include <tlhelp32.h>
#include <psapi.h>
#include <thread>
#include <atomic>
#include <map>
#include <memory>
#include <string>
#include <math.h>
#include "../shared/p2overlay_ipc.h"

struct Session {
  DWORD pid = 0;
  HANDLE mapping = nullptr;
  HANDLE frameReady = nullptr;
  HANDLE lock = nullptr;
  HANDLE pipe = INVALID_HANDLE_VALUE;
  uint8_t* base = nullptr;
  P2OverlayHeader* header = nullptr;
  std::thread pipeThread;
  std::atomic<bool> running{ false };
  Napi::ThreadSafeFunction onInput;
};

static std::map<DWORD, std::shared_ptr<Session>> g_sessions;

static std::shared_ptr<Session> findSession(const Napi::CallbackInfo& info) {
  if (info.Length() < 1 || !info[0].IsNumber()) return nullptr;
  DWORD pid = info[0].As<Napi::Number>().Uint32Value();
  auto it = g_sessions.find(pid);
  return it == g_sessions.end() ? nullptr : it->second;
}

// Başlık mutex'i: DLL ile ortak (kısayol tablosu gibi çok alanlı yazımlar için).
static bool lockHeader(Session& s, DWORD ms = 50) {
  if (!s.lock) return false;
  DWORD r = WaitForSingleObject(s.lock, ms);
  return r == WAIT_OBJECT_0 || r == WAIT_ABANDONED;
}
static void unlockHeader(Session& s, bool locked) { if (locked) ReleaseMutex(s.lock); }

static void pipeLoop(std::shared_ptr<Session> s) {
  wchar_t name[128]; swprintf(name, 128, L"\\\\.\\pipe\\P2Overlay_Input_%lu", s->pid);
  while (s->running) {
    HANDLE pipe = CreateNamedPipeW(name, PIPE_ACCESS_INBOUND, PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT, 1, 0, 64 * 1024, 0, nullptr);
    if (pipe == INVALID_HANDLE_VALUE) { Sleep(200); continue; }
    s->pipe = pipe;
    BOOL connected = ConnectNamedPipe(pipe, nullptr) ? TRUE : (GetLastError() == ERROR_PIPE_CONNECTED);
    if (connected) {
      P2InputMsg msg; DWORD read = 0; // sabit 24 baytlık kayıt
      while (s->running && ReadFile(pipe, &msg, sizeof(msg), &read, nullptr) && read == sizeof(msg)) {
        // Kopya heap'te olmalı: BlockingCall JS tarafına asenkron teslim eder; yığın değişkeni
        // bir sonraki ReadFile'da ezilir ve tüm olaylar son mesajın içeriğiyle gelirdi.
        P2InputMsg* copy = new P2InputMsg(msg);
        napi_status st = s->onInput.BlockingCall(copy, [](Napi::Env env, Napi::Function cb, P2InputMsg* m) {
          Napi::Object o = Napi::Object::New(env);
          o.Set("type", m->type); o.Set("flags", m->flags); o.Set("x", m->x); o.Set("y", m->y);
          o.Set("wheelDelta", m->wheelDelta); o.Set("key", m->key);
          delete m;
          if (env != nullptr) cb.Call({ o });
        });
        if (st != napi_ok) delete copy;
      }
    }
    DisconnectNamedPipe(pipe); CloseHandle(pipe); s->pipe = INVALID_HANDLE_VALUE;
  }
}

// JS kısayol listesi [{id, vk, mods, passThrough}] -> başlık tablosu. Fazlası (8+) atılır,
// geçersiz girdiler (id/vk 0) atlanır. id 1 hotkeyVk/hotkeyMods'a da aynalanır (geriye uyum).
static void writeHotkeyTable(P2OverlayHeader* h, Napi::Value list) {
  P2Hotkey table[P2_OVL_MAX_HOTKEYS] = {};
  int n = 0;
  if (list.IsArray()) {
    Napi::Array arr = list.As<Napi::Array>();
    for (uint32_t i = 0; i < arr.Length() && n < P2_OVL_MAX_HOTKEYS; i++) {
      Napi::Value v = arr.Get(i); if (!v.IsObject()) continue;
      Napi::Object o = v.As<Napi::Object>();
      uint32_t id = o.Has("id") && o.Get("id").IsNumber() ? o.Get("id").As<Napi::Number>().Uint32Value() : 0;
      uint32_t vk = o.Has("vk") && o.Get("vk").IsNumber() ? o.Get("vk").As<Napi::Number>().Uint32Value() : 0;
      uint32_t mods = o.Has("mods") && o.Get("mods").IsNumber() ? o.Get("mods").As<Napi::Number>().Uint32Value() : 0;
      bool pass = o.Has("passThrough") && o.Get("passThrough").IsBoolean() && o.Get("passThrough").As<Napi::Boolean>().Value();
      if (!id || !vk || id > 255 || vk > 0xFFFF) continue;
      table[n].id = (uint8_t)id; table[n].vk = (uint16_t)vk;
      table[n].mods = (uint8_t)((mods & 0x7) | (pass ? P2_HOTKEY_PASSTHRU : 0));
      n++;
    }
  }
  memcpy(h->hotkeys, table, sizeof(table));
  for (int i = 0; i < n; i++) if (table[i].id == P2_HOTKEY_PANEL) { h->hotkeyVk = table[i].vk; h->hotkeyMods = table[i].mods & 0x7; break; }
}

// create(pid, {hotkeys:[{id,vk,mods,passThrough}]}, onInput) -> bool
// create(pid, hotkeyVk, hotkeyMods, onInput) -> bool   (eski konumsal imza, korunur)
Napi::Value Create(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  if (info.Length() < 3 || !info[0].IsNumber()) return Napi::Boolean::New(env, false);
  DWORD pid = info[0].As<Napi::Number>().Uint32Value();
  bool optsForm = info[1].IsObject() && !info[1].IsFunction();
  Napi::Value cb = optsForm ? info[2] : (info.Length() > 3 ? info[3] : env.Undefined());
  if (!cb.IsFunction()) return Napi::Boolean::New(env, false);
  if (g_sessions.count(pid)) return Napi::Boolean::New(env, false); // aynı pid için ikinci oturum yok
  auto s = std::make_shared<Session>();
  s->pid = pid;
  wchar_t name[128];
  swprintf(name, 128, L"Local\\P2Overlay_Frame_%lu", pid);
  s->mapping = CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0, p2_total_mapping_bytes(), name);
  if (!s->mapping) return Napi::Boolean::New(env, false);
  s->base = (uint8_t*)MapViewOfFile(s->mapping, FILE_MAP_ALL_ACCESS, 0, 0, p2_total_mapping_bytes());
  if (!s->base) { CloseHandle(s->mapping); return Napi::Boolean::New(env, false); }
  s->header = (P2OverlayHeader*)s->base;
  memset(s->header, 0, sizeof(P2OverlayHeader));
  s->header->magic = P2_OVL_MAGIC; s->header->schema = P2_OVL_SCHEMA; s->header->format = 0;
  if (optsForm) {
    Napi::Object opts = info[1].As<Napi::Object>();
    writeHotkeyTable(s->header, opts.Has("hotkeys") ? opts.Get("hotkeys") : env.Undefined());
    s->header->hotkeyGen = 1;
  } else {
    // Eski imza: yalnız panel kısayolu; tabloya id 1 olarak da yazılır (DLL tablo boşsa zaten buna düşer).
    uint32_t vk = info[1].IsNumber() ? info[1].As<Napi::Number>().Uint32Value() : 0;
    uint32_t mods = info[2].IsNumber() ? info[2].As<Napi::Number>().Uint32Value() : 0;
    s->header->hotkeyVk = vk; s->header->hotkeyMods = mods & 0x7;
    if (vk) { s->header->hotkeys[0].id = P2_HOTKEY_PANEL; s->header->hotkeys[0].vk = (uint16_t)vk; s->header->hotkeys[0].mods = (uint8_t)(mods & 0x7); }
    s->header->hotkeyGen = 1;
  }
  swprintf(name, 128, L"Local\\P2Overlay_FrameReady_%lu", pid);
  s->frameReady = CreateEventW(nullptr, FALSE, FALSE, name);
  swprintf(name, 128, L"Local\\P2Overlay_Lock_%lu", pid);
  s->lock = CreateMutexW(nullptr, FALSE, name);
  s->onInput = Napi::ThreadSafeFunction::New(env, cb.As<Napi::Function>(), "p2overlayInput", 0, 1);
  s->running = true;
  s->pipeThread = std::thread(pipeLoop, s);
  g_sessions[pid] = s;
  return Napi::Boolean::New(env, true);
}

// setHotkeys(pid, [{id,vk,mods,passThrough}]) -> bool : tabloyu mutex altında yeniden yazar, hotkeyGen++.
Napi::Value SetHotkeys(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  auto s = findSession(info); if (!s) return Napi::Boolean::New(env, false);
  bool locked = lockHeader(*s);
  writeHotkeyTable(s->header, info.Length() > 1 ? info[1] : env.Undefined());
  s->header->hotkeyGen++;
  unlockHeader(*s, locked);
  return Napi::Boolean::New(env, true);
}

// writeFrame(pid, buffer, fullW, fullH, dx, dy, dw, dh) -> bool
// Tek kalıcı yuva (0): host her zaman yuva 0'a yazar, yuva tam ve güncel kareyi taşır.
//  - buffer.length >= fullW*fullH*4  -> TAM kare: yuvanın tamamı kopyalanır (boyut değişiminde de bu yol).
//  - aksi hâlde buffer tam olarak dw*dh*4 bayt olmalı ve fullW/fullH başlıktaki boyutla aynı olmalı:
//    satır satır (dx,dy) konumuna kopyalanır (kareye kırpılır). Yuvada henüz tam kare yoksa reddedilir.
// Kopya + başlık yazımı mutex altındadır (DLL yarım dikdörtgen okumaz). Başlık: width/height/stride,
// dirty = DLL son kareyi ack'lediyse yeni dikdörtgen, değilse önceki dirty ile BİRLEŞİM; frameId++.
Napi::Value WriteFrame(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  auto s = findSession(info); if (!s) return Napi::Boolean::New(env, false);
  if (info.Length() < 4 || !info[1].IsBuffer()) return Napi::Boolean::New(env, false);
  Napi::Buffer<uint8_t> buf = info[1].As<Napi::Buffer<uint8_t>>();
  uint32_t w = info[2].As<Napi::Number>().Uint32Value();
  uint32_t h = info[3].As<Napi::Number>().Uint32Value();
  if (!w || !h || w > P2_OVL_MAX_W || h > P2_OVL_MAX_H) return Napi::Boolean::New(env, false);
  int32_t dx = info.Length() > 4 && info[4].IsNumber() ? info[4].As<Napi::Number>().Int32Value() : 0;
  int32_t dy = info.Length() > 5 && info[5].IsNumber() ? info[5].As<Napi::Number>().Int32Value() : 0;
  int32_t dw = info.Length() > 6 && info[6].IsNumber() ? info[6].As<Napi::Number>().Int32Value() : 0;
  int32_t dh = info.Length() > 7 && info[7].IsNumber() ? info[7].As<Napi::Number>().Int32Value() : 0;
  P2OverlayHeader* hd = s->header;
  uint8_t* slot = s->base + p2_frame_slot_offset(0);
  const uint32_t stride = w * 4;
  size_t fullBytes = (size_t)w * h * 4;
  bool full = buf.Length() >= fullBytes;
  // Kısmi yazım ön denetimi (kilitsiz; boyut yalnız bu fonksiyonda değişir).
  int32_t cx = dx, cy = dy, cw = dw, ch = dh; // kırpılmış dikdörtgen
  if (!full) {
    if (dw <= 0 || dh <= 0) return Napi::Boolean::New(env, false);
    if (buf.Length() != (size_t)dw * (size_t)dh * 4) return Napi::Boolean::New(env, false);
    if (hd->frameId == 0 || hd->width != w || hd->height != h) return Napi::Boolean::New(env, false); // önce tam kare gerekir
    if (!p2_rect_clip(&cx, &cy, &cw, &ch, w, h)) return Napi::Boolean::New(env, false);
  }
  bool locked = lockHeader(*s);
  if (full) {
    memcpy(slot, buf.Data(), fullBytes);
  } else {
    // Kaynak satır adımı dw*4; kırpılmış dikdörtgenin kaynaktaki konumu (cx-dx, cy-dy).
    const uint8_t* src = buf.Data() + (size_t)(cy - dy) * (size_t)dw * 4 + (size_t)(cx - dx) * 4;
    uint8_t* dst = slot + (size_t)cy * stride + (size_t)cx * 4;
    size_t rowBytes = (size_t)cw * 4;
    for (int32_t y = 0; y < ch; y++) memcpy(dst + (size_t)y * stride, src + (size_t)y * (size_t)dw * 4, rowBytes);
  }
  // Ack: DLL son frameId'yi yüklediyse dikdörtgen sıfırdan başlar; yoksa kaçırdığı bölgelerle birleşir.
  bool consumed = hd->reserved2[P2_OVL_ACK_FRAME_SLOT] == (uint32_t)hd->frameId;
  int32_t nx, ny, nw, nh;
  if (full) { nx = 0; ny = 0; nw = (int32_t)w; nh = (int32_t)h; }
  else if (consumed) { nx = cx; ny = cy; nw = cw; nh = ch; }
  else if (p2_rect_is_full(hd->dirtyX, hd->dirtyY, hd->dirtyW, hd->dirtyH, w, h)) { nx = 0; ny = 0; nw = (int32_t)w; nh = (int32_t)h; }
  else {
    int64_t x0 = hd->dirtyX < cx ? hd->dirtyX : cx, y0 = hd->dirtyY < cy ? hd->dirtyY : cy;
    int64_t ax1 = (int64_t)hd->dirtyX + hd->dirtyW, bx1 = (int64_t)cx + cw, ay1 = (int64_t)hd->dirtyY + hd->dirtyH, by1 = (int64_t)cy + ch;
    int64_t x1 = ax1 > bx1 ? ax1 : bx1, y1 = ay1 > by1 ? ay1 : by1;
    nx = (int32_t)x0; ny = (int32_t)y0; nw = (int32_t)(x1 - x0); nh = (int32_t)(y1 - y0);
  }
  hd->width = w; hd->height = h; hd->stride = stride;
  hd->dirtyX = nx; hd->dirtyY = ny; hd->dirtyW = nw; hd->dirtyH = nh;
  hd->frontIndex = 0;
  hd->frameId++;
  unlockHeader(*s, locked);
  SetEvent(s->frameReady);
  return Napi::Boolean::New(env, true);
}

// setVisible(pid, bool): header->visible tek doğruluk kaynağı; DLL bir tik içinde imleci bırakır/alır.
Napi::Value SetVisible(const Napi::CallbackInfo& info) {
  auto s = findSession(info);
  if (s && info.Length() > 1) s->header->visible = info[1].ToBoolean().Value() ? 1 : 0;
  return info.Env().Undefined();
}

// setDrawMask(pid, mask): P2_OVL_DRAW_* bitleri; panel kapalıyken de kare çizilir, girdi oyunda kalır.
Napi::Value SetDrawMask(const Napi::CallbackInfo& info) {
  auto s = findSession(info);
  if (s && info.Length() > 1 && info[1].IsNumber()) s->header->drawMask = info[1].As<Napi::Number>().Uint32Value() & 0xF;
  return info.Env().Undefined();
}

// setCursor(pid, id): Win32 IDC_* kaynak kimliği (32512 ok, 32513 I-beam, 32644 yatay çift ok…); 0 = ok.
Napi::Value SetCursorShape(const Napi::CallbackInfo& info) {
  auto s = findSession(info);
  if (s && info.Length() > 1 && info[1].IsNumber()) s->header->reserved2[P2_OVL_CURSOR_SLOT] = info[1].As<Napi::Number>().Uint32Value();
  return info.Env().Undefined();
}

// setNotify(pid, bool): geriye uyum takma adı; yalnız bit0 (TOAST) kurar/temizler.
Napi::Value SetNotify(const Napi::CallbackInfo& info) {
  auto s = findSession(info);
  if (s && info.Length() > 1) {
    bool on = info[1].ToBoolean().Value();
    s->header->drawMask = on ? (s->header->drawMask | P2_OVL_DRAW_TOAST) : (s->header->drawMask & ~P2_OVL_DRAW_TOAST);
  }
  return info.Env().Undefined();
}

// visible(pid) -> bool  (DLL toggle'ını okur)
Napi::Value Visible(const Napi::CallbackInfo& info) {
  auto s = findSession(info);
  return Napi::Boolean::New(info.Env(), s && s->header->visible != 0);
}

// frames(pid) -> number  (DLL'in Present sayacı; >0 = oyun penceresi çizmeye başladı)
Napi::Value Frames(const Napi::CallbackInfo& info) {
  auto s = findSession(info);
  return Napi::Number::New(info.Env(), s ? (double)s->header->frames : 0);
}

// requestShot(pid) -> number : shotRequestId++ (mutex altında), yeni id döner. 0 = oturum yok.
// DLL (Faz 4) kopyayı bitirince shotReadyId = id + shotW/H/Stride/Format + SHOT_READY mesajı.
Napi::Value RequestShot(const Napi::CallbackInfo& info) {
  auto s = findSession(info); if (!s) return Napi::Number::New(info.Env(), 0);
  bool locked = lockHeader(*s);
  s->header->shotFlags = 0;
  uint32_t id = ++s->header->shotRequestId;
  unlockHeader(*s, locked);
  return Napi::Number::New(info.Env(), (double)id);
}

// IEEE half -> float
static inline float halfToFloat(uint16_t h) {
  uint32_t sign = (uint32_t)(h >> 15) << 31, exp = (h >> 10) & 0x1F, man = h & 0x3FF, bits;
  if (exp == 0) {
    if (!man) bits = sign;
    else { exp = 127 - 15 + 1; while (!(man & 0x400)) { man <<= 1; exp--; } man &= 0x3FF; bits = sign | (exp << 23) | (man << 13); }
  } else if (exp == 31) bits = sign | 0x7F800000u | (man << 13);
  else bits = sign | ((exp + 127 - 15) << 23) | (man << 13);
  float f; memcpy(&f, &bits, 4); return f;
}
static inline uint8_t unitToByte(float v) { if (!(v > 0.f)) return 0; if (v >= 1.f) return 255; return (uint8_t)(v * 255.f + 0.5f); }

// readShot(pid) -> {id,width,height,buffer} | null
// Ham yuva -> sıkı BGRA8 (alfa 255). Desteklenen DXGI formatları: 87/91 BGRA8(_SRGB), 28/29 RGBA8(_SRGB),
// 24 R10G10B10A2_UNORM, 10 RGBA16F (clamp 0..1). Başka format / başarısız bayrağı / hazır değil -> null.
Napi::Value ReadShot(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  auto s = findSession(info); if (!s) return env.Null();
  P2OverlayHeader* h = s->header;
  bool locked = lockHeader(*s);
  uint32_t req = h->shotRequestId, ready = h->shotReadyId, flags = h->shotFlags;
  uint32_t w = h->shotW, hh = h->shotH, stride = h->shotStride, fmt = h->shotFormat;
  unlockHeader(*s, locked);
  if (!req || ready != req || (flags & 1)) return env.Null();
  if (!w || !hh || w > P2_OVL_MAX_W || hh > P2_OVL_MAX_H) return env.Null();
  uint32_t bpp = 0;
  switch (fmt) { case 87: case 91: case 28: case 29: case 24: bpp = 4; break; case 10: bpp = 8; break; default: return env.Null(); }
  if (stride < w * bpp || (uint64_t)stride * hh > (uint64_t)P2_OVL_SHOT_BYTES) return env.Null();
  const uint8_t* src = s->base + p2_shot_slot_offset();
  Napi::Buffer<uint8_t> out = Napi::Buffer<uint8_t>::New(env, (size_t)w * hh * 4);
  uint8_t* dst = out.Data();
  for (uint32_t y = 0; y < hh; y++) {
    const uint8_t* row = src + (size_t)y * stride; uint8_t* d = dst + (size_t)y * w * 4;
    switch (fmt) {
      case 87: case 91: // BGRA8: kopyala, alfa 255
        for (uint32_t x = 0; x < w; x++) { d[0] = row[0]; d[1] = row[1]; d[2] = row[2]; d[3] = 255; d += 4; row += 4; }
        break;
      case 28: case 29: // RGBA8: R/B takas
        for (uint32_t x = 0; x < w; x++) { d[0] = row[2]; d[1] = row[1]; d[2] = row[0]; d[3] = 255; d += 4; row += 4; }
        break;
      case 24: { // R10G10B10A2: üst 8 bit
        for (uint32_t x = 0; x < w; x++) { uint32_t p; memcpy(&p, row, 4); d[0] = (uint8_t)(((p >> 20) & 0x3FF) >> 2); d[1] = (uint8_t)(((p >> 10) & 0x3FF) >> 2); d[2] = (uint8_t)((p & 0x3FF) >> 2); d[3] = 255; d += 4; row += 4; }
        break; }
      case 10: { // RGBA16F: half -> clamp 0..1 -> 8 bit (HDR ton eşleme yok, kırpma)
        for (uint32_t x = 0; x < w; x++) { uint16_t c[4]; memcpy(c, row, 8); d[0] = unitToByte(halfToFloat(c[2])); d[1] = unitToByte(halfToFloat(c[1])); d[2] = unitToByte(halfToFloat(c[0])); d[3] = 255; d += 4; row += 8; }
        break; }
    }
  }
  Napi::Object o = Napi::Object::New(env);
  o.Set("id", (double)ready); o.Set("width", (double)w); o.Set("height", (double)hh); o.Set("buffer", out);
  return o;
}

// stats(pid) -> {frames, visible, drawMask, hotkeyGen, shotRequestId, shotReadyId, shotFlags, width, height, inputFocus,
//                frameId, ackFrameId, dirty:{x,y,w,h}} | null
// ackFrameId: DLL'in yüklediği son frameId (düşük 32 bit, reserved2[0]); dirty: son ack'ten beri biriken dikdörtgen.
Napi::Value Stats(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  auto s = findSession(info); if (!s) return env.Null();
  P2OverlayHeader* h = s->header;
  Napi::Object o = Napi::Object::New(env);
  o.Set("frames", (double)h->frames); o.Set("visible", h->visible != 0); o.Set("drawMask", (double)h->drawMask);
  o.Set("hotkeyGen", (double)h->hotkeyGen); o.Set("shotRequestId", (double)h->shotRequestId); o.Set("shotReadyId", (double)h->shotReadyId);
  o.Set("shotFlags", (double)h->shotFlags); o.Set("width", (double)h->width); o.Set("height", (double)h->height);
  o.Set("inputFocus", (double)h->inputFocus);
  o.Set("frameId", (double)h->frameId); o.Set("ackFrameId", (double)h->reserved2[P2_OVL_ACK_FRAME_SLOT]);
  Napi::Object d = Napi::Object::New(env);
  d.Set("x", (double)h->dirtyX); d.Set("y", (double)h->dirtyY); d.Set("w", (double)h->dirtyW); d.Set("h", (double)h->dirtyH);
  o.Set("dirty", d);
  return o;
}

// windowTitle(pid) -> string : pid'in en büyük görünür, sahipsiz üst penceresinin başlığı ('' yoksa).
// desktopCapturer yedeği (ekran görüntüsü DLL'den gelmezse pencere adıyla kaynak seçimi) için.
Napi::Value WindowTitle(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  if (info.Length() < 1 || !info[0].IsNumber()) return Napi::String::New(env, "");
  struct Ctx { DWORD pid; HWND hwnd; long long area; } ctx{ info[0].As<Napi::Number>().Uint32Value(), nullptr, 0 };
  EnumWindows([](HWND hw, LPARAM l) -> BOOL {
    auto* c = (Ctx*)l; DWORD wp = 0; GetWindowThreadProcessId(hw, &wp);
    if (wp != c->pid || !IsWindowVisible(hw) || GetWindow(hw, GW_OWNER)) return TRUE;
    RECT r{}; GetWindowRect(hw, &r); long long area = (long long)(r.right - r.left) * (long long)(r.bottom - r.top);
    if (area > c->area) { c->area = area; c->hwnd = hw; }
    return TRUE; }, (LPARAM)&ctx);
  if (!ctx.hwnd) return Napi::String::New(env, "");
  wchar_t title[512] = {0}; int n = GetWindowTextW(ctx.hwnd, title, 512); if (n < 0) n = 0;
  return Napi::String::New(env, std::u16string((const char16_t*)title, (size_t)n));
}

// processes() -> [{pid, ppid, name, wow64}]  Toolhelp anlık görüntüsü; alt süreç takibi için
// (Unreal: kök exe başlatıcı, asıl oyun Binaries/Win64 altındaki alt süreç).
Napi::Value Processes(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  Napi::Array list = Napi::Array::New(env);
  HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snap == INVALID_HANDLE_VALUE) return list;
  PROCESSENTRY32W entry{}; entry.dwSize = sizeof(entry);
  uint32_t index = 0;
  if (Process32FirstW(snap, &entry)) {
    do {
      Napi::Object o = Napi::Object::New(env);
      o.Set("pid", (double)entry.th32ProcessID);
      o.Set("ppid", (double)entry.th32ParentProcessID);
      o.Set("name", Napi::String::New(env, std::u16string((const char16_t*)entry.szExeFile)));
      BOOL wow64 = FALSE;
      HANDLE h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, entry.th32ProcessID);
      if (h) { IsWow64Process(h, &wow64); CloseHandle(h); }
      o.Set("wow64", wow64 ? true : false);
      list.Set(index++, o);
    } while (Process32NextW(snap, &entry));
  }
  CloseHandle(snap);
  return list;
}

// processStats(pid) -> {cpuMs, wallMs, ramMb, cores} | null : oyun sürecinin toplam CPU süresi (çekirdek+kullanıcı, ms),
// çağrı anı (ms), çalışma belleği (MB), mantıksal çekirdek sayısı. CPU yüzdesi host'ta iki örnek farkından hesaplanır:
// (ΔcpuMs / ΔwallMs) / cores * 100. Yalnız PROCESS_QUERY_LIMITED_INFORMATION ister (yönetici gerekmez).
Napi::Value ProcessStats(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  if (info.Length() < 1 || !info[0].IsNumber()) return env.Null();
  DWORD pid = info[0].As<Napi::Number>().Uint32Value();
  HANDLE h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
  if (!h) return env.Null();
  FILETIME created{}, exited{}, kernel{}, user{};
  BOOL okTimes = GetProcessTimes(h, &created, &exited, &kernel, &user);
  PROCESS_MEMORY_COUNTERS mem{}; mem.cb = sizeof(mem);
  BOOL okMem = GetProcessMemoryInfo(h, &mem, sizeof(mem));
  CloseHandle(h);
  if (!okTimes) return env.Null();
  auto toMs = [](const FILETIME& ft) { ULARGE_INTEGER v; v.LowPart = ft.dwLowDateTime; v.HighPart = ft.dwHighDateTime; return (double)v.QuadPart / 10000.0; };
  FILETIME now{}; GetSystemTimeAsFileTime(&now);
  SYSTEM_INFO si{}; GetSystemInfo(&si);
  Napi::Object o = Napi::Object::New(env);
  o.Set("cpuMs", toMs(kernel) + toMs(user));
  o.Set("wallMs", toMs(now));
  o.Set("ramMb", okMem ? (double)(mem.WorkingSetSize / 1048576.0) : -1.0);
  o.Set("cores", (double)(si.dwNumberOfProcessors ? si.dwNumberOfProcessors : 1));
  return o;
}

// destroy(pid)
Napi::Value Destroy(const Napi::CallbackInfo& info) {
  auto s = findSession(info); if (!s) return info.Env().Undefined();
  DWORD pid = s->pid;
  s->running = false;
  if (s->pipe != INVALID_HANDLE_VALUE) { HANDLE dummy = CreateFileW((L"\\\\.\\pipe\\P2Overlay_Input_" + std::to_wstring(pid)).c_str(), GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, 0, nullptr); if (dummy != INVALID_HANDLE_VALUE) CloseHandle(dummy); }
  if (s->pipeThread.joinable()) s->pipeThread.join();
  s->onInput.Release();
  if (s->base) UnmapViewOfFile(s->base);
  if (s->mapping) CloseHandle(s->mapping);
  if (s->frameReady) CloseHandle(s->frameReady);
  if (s->lock) CloseHandle(s->lock);
  g_sessions.erase(pid);
  return info.Env().Undefined();
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
  exports.Set("create", Napi::Function::New(env, Create));
  exports.Set("writeFrame", Napi::Function::New(env, WriteFrame));
  exports.Set("setVisible", Napi::Function::New(env, SetVisible));
  exports.Set("setDrawMask", Napi::Function::New(env, SetDrawMask));
  exports.Set("setNotify", Napi::Function::New(env, SetNotify));
  exports.Set("setCursor", Napi::Function::New(env, SetCursorShape));
  exports.Set("setHotkeys", Napi::Function::New(env, SetHotkeys));
  exports.Set("requestShot", Napi::Function::New(env, RequestShot));
  exports.Set("readShot", Napi::Function::New(env, ReadShot));
  exports.Set("stats", Napi::Function::New(env, Stats));
  exports.Set("windowTitle", Napi::Function::New(env, WindowTitle));
  exports.Set("processStats", Napi::Function::New(env, ProcessStats));
  exports.Set("visible", Napi::Function::New(env, Visible));
  exports.Set("frames", Napi::Function::New(env, Frames));
  exports.Set("processes", Napi::Function::New(env, Processes));
  exports.Set("destroy", Napi::Function::New(env, Destroy));
  exports.Set("schema", Napi::Number::New(env, P2_OVL_SCHEMA));
  // Kısmi kare yolu: writeFrame yalnız kirli dikdörtgeni (dw*dh*4 bayt) kabul eder; JS tarafı bunu görünce
  // image.crop(dirty).toBitmap() gönderir, eski eklentide tam kare göndermeye devam eder.
  exports.Set("partialFrames", Napi::Boolean::New(env, true));
  return exports;
}
NODE_API_MODULE(p2overlayhost, Init)
