// P2 oyun içi overlay — paylaşılan IPC sözleşmesi.
// Bu başlık hem enjekte edilen DLL (native/overlay/dll) hem de Electron tarafındaki
// host eklentisi (native/overlay/host) tarafından kullanılır. Yapı düzeni iki taraf
// arasında birebir aynı olmalıdır; alan eklerken SCHEMA_VERSION'ı artır.
// Şema 4 (2026-10): drawMask (eski notify), kısayol tablosu, ekran görüntüsü alanları, HOTKEY/SHOT_READY mesajları.
#pragma once
#include <stdint.h>

#define P2_OVL_MAGIC 0x50324F56u   // 'P2OV'
#define P2_OVL_SCHEMA 4

// Paylaşılan bellek adları (pid ile eklenir): hepsi "Local\\" ad alanında.
//   Frame mapping:  P2Overlay_Frame_<pid>
//   Frame-ready ev: P2Overlay_FrameReady_<pid>
//   Header mutex:   P2Overlay_Lock_<pid>
//   Input pipe:     \\.\pipe\P2Overlay_Input_<pid>

// İki kare yuvası + bir ekran görüntüsü yuvası. 4K'ya kadar: 3840*2160*4 ~= 33 MB/yuva.
// Ekran görüntüsü yuvası 2 kat: RGBA16F (8 bayt/px) kaynaklar 4K'da 66 MB ister.
//
// Kısmi kare yolu (şema 4, düzen değişmedi): host artık ÇİFT TAMPON KULLANMAZ. Yuva 0 tek ve kalıcı
// tam karedir (frontIndex hep 0); her Chromium boyamasında yalnız kirli dikdörtgen yuvaya yazılır,
// `dirtyX/Y/W/H` o dikdörtgeni taşır ve frameId++. Kare yazımı (tam ya da dikdörtgen) başlık mutex'i
// altında yapılır; DLL de dikdörtgeni GPU'ya kopyalarken mutex'i tutar (yarım yazılmış dikdörtgen
// okunmaz). `dirtyW/H == 0` ya da dikdörtgen kareyi kaplıyorsa tam yükleme.
// Alındı (ack): DLL yüklediği son frameId'nin düşük 32 bitini `reserved2[P2_OVL_ACK_FRAME_SLOT]`'a
// yazar (mutex altında, kopyayla birlikte). Host yazmadan önce `ack == (uint32_t)frameId` ise DLL
// son kareyi işlemiştir -> yeni dikdörtgen; değilse DLL arada kareleri kaçırmıştır -> başlıktaki
// dikdörtgenle BİRLEŞİM yazılır. Böylece iki Present arasında kaç kare gelirse gelsin DLL tek
// seferde kaçırdığı bölgelerin tamamını yükler.
#define P2_OVL_ACK_FRAME_SLOT 0   // reserved2[0]: DLL'in yüklediği son frameId (düşük 32 bit)
// İmleç şekli: host Chromium'un cursor-changed olayını Win32 IDC_* kaynak kimliğine çevirip reserved2[1]'e
// yazar (0 = ok imleci). DLL overlay açıkken SetCursor/WM_SETCURSOR'da bu şekli kullanır; böylece
// kenar tutamaçlarında çift ok, metin alanında I-beam, bağlantıda el imleci oyunda da görünür.
#define P2_OVL_CURSOR_SLOT 1
#define P2_OVL_MAX_W 3840
#define P2_OVL_MAX_H 2160
#define P2_OVL_SLOT_BYTES (P2_OVL_MAX_W * P2_OVL_MAX_H * 4)
#define P2_OVL_SHOT_BYTES (P2_OVL_SLOT_BYTES * 2)

// drawMask bitleri (host yazar): panel kapalıyken bile kare çizilir, girdi yakalanmaz.
#define P2_OVL_DRAW_TOAST  0x1u
#define P2_OVL_DRAW_PINNED 0x2u
#define P2_OVL_DRAW_HUD    0x4u
#define P2_OVL_DRAW_DEBUG  0x8u

// Kısayol kimlikleri (src/overlay/overlayTypes.ts HOTKEY_ID ile aynı).
#define P2_HOTKEY_PANEL      1
#define P2_HOTKEY_SCREENSHOT 2
#define P2_HOTKEY_MUTE       3
#define P2_HOTKEY_PTT        4
#define P2_HOTKEY_DEAFEN     5
#define P2_OVL_MAX_HOTKEYS   8

// mods: bit0 shift, bit1 ctrl, bit2 alt; bit7 = pass-through (tuş oyuna da iletilir, bas-konuş için).
#define P2_HOTKEY_MOD_SHIFT 0x01
#define P2_HOTKEY_MOD_CTRL  0x02
#define P2_HOTKEY_MOD_ALT   0x04
#define P2_HOTKEY_PASSTHRU  0x80

#pragma pack(push, 4)
struct P2Hotkey {
  uint16_t vk;    // Win32 VK; 0 = boş
  uint8_t  mods;  // P2_HOTKEY_MOD_* | P2_HOTKEY_PASSTHRU
  uint8_t  id;    // P2_HOTKEY_*; 0 = boş
};

struct P2OverlayHeader {
  uint32_t magic;        // P2_OVL_MAGIC
  uint32_t schema;       // P2_OVL_SCHEMA
  uint32_t width;        // overlay yüzeyi genişliği (px)
  uint32_t height;       // overlay yüzeyi yüksekliği (px)
  uint32_t stride;       // satır başına bayt (width*4, BGRA)
  uint32_t format;       // 0 = BGRA8
  uint32_t frontIndex;   // DLL'in okuyacağı yuva: kısmi kare yolunda hep 0 (tek kalıcı yuva)
  uint32_t visible;      // TEK DOĞRULUK KAYNAĞI: 0/1 overlay görünür mü (DLL kısayolda, host kapatırken yazar)
  uint64_t frameId;      // monoton artan; DLL yalnız değişince yeniden yükler
  int32_t  dirtyX, dirtyY, dirtyW, dirtyH; // kirli dikdörtgen: son ack'ten beri yazılan bölgelerin birleşimi (W/H 0 ya da tam kaplama = tam)
  uint32_t shotRequestId; // host artırır: DLL arka tamponu kopyalasın
  uint32_t shotReadyId;   // DLL ayarlar: ekran görüntüsü hazır (== shotRequestId)
  uint32_t hotkeyVk;     // panel kısayolu aynası (geriye uyum; tablo id 1 ile aynı)
  uint32_t hotkeyMods;   // bit0 shift, bit1 ctrl, bit2 alt
  uint32_t drawMask;     // host yazar: P2_OVL_DRAW_* bitleri; DLL (visible || drawMask) iken çizer
  uint32_t frames;       // DLL yazar: Present sayisi (ilk kare = oyun penceresi cizmeye basladi)
  uint32_t reserved[6];
  // ---- şema 4 ekleri ----
  uint32_t hotkeyGen;    // host her tablo yazımında artırır; DLL değişince tabloyu yeniden yükler
  P2Hotkey hotkeys[P2_OVL_MAX_HOTKEYS];
  uint32_t shotW, shotH, shotStride, shotFormat; // DLL yazar; shotFormat = DXGI_FORMAT (ham kopya)
  uint32_t shotFlags;    // bit0 = yakalama başarısız (host desktopCapturer'a düşer)
  uint32_t inputFocus;   // DLL yazar: oyun penceresi etkin mi (WM_ACTIVATE)
  uint32_t reserved2[8]; // [P2_OVL_ACK_FRAME_SLOT] = DLL'in yüklediği son frameId (düşük 32 bit); [P2_OVL_CURSOR_SLOT] = imleç IDC_* kimliği; kalanı 0
  // Ardından: 2 kare yuvası (kısmi kare yolunda yalnız yuva 0 kullanılır) (P2_OVL_SLOT_BYTES) + 1 ekran görüntüsü yuvası (P2_OVL_SHOT_BYTES).
};
#pragma pack(pop)

// Girdi mesajı (DLL -> host, adlandırılmış boru, sabit 24 bayt kayıt).
enum P2InputType {
  P2_INPUT_MOUSE_MOVE = 1,
  P2_INPUT_MOUSE_BTN  = 2,
  P2_INPUT_MOUSE_WHEEL = 3, // x,y = istemci koordinatı; flags bit8 = yatay tekerlek
  P2_INPUT_KEY        = 4,  // flags bit0 down; bit8 = otomatik tekrar
  P2_INPUT_CHAR       = 5,  // key = UTF-16 birimi (surrogate çiftleri host birleştirir)
  P2_INPUT_TOGGLE     = 6,  // panel kısayolu: overlay aç/kapa (flags bit0 = yeni görünürlük)
  P2_INPUT_FOCUS      = 7,  // oyun odak kazandı/kaybetti (flags bit0)
  P2_INPUT_RESIZE     = 8,  // swapchain boyutu değişti (x=w, y=h)
  P2_INPUT_HOTKEY     = 9,  // key = kısayol id, flags bit0 down, x = vk, y = mods
  P2_INPUT_SHOT_READY = 10, // key = requestId, x = w, y = h, wheelDelta = format, flags bit0 = başarısız
};

#define P2_INPUT_FLAG_DOWN   0x0001
#define P2_INPUT_FLAG_SHIFT  0x0002
#define P2_INPUT_FLAG_CTRL   0x0004
#define P2_INPUT_FLAG_ALT    0x0008
#define P2_INPUT_FLAG_META   0x0010
#define P2_INPUT_FLAG_LEFT   0x0020
#define P2_INPUT_FLAG_RIGHT  0x0040
#define P2_INPUT_FLAG_MIDDLE 0x0080
#define P2_INPUT_FLAG_REPEAT 0x0100 // KEY: otomatik tekrar
#define P2_INPUT_FLAG_HWHEEL 0x0100 // WHEEL: yatay

#pragma pack(push, 1)
struct P2InputMsg {
  uint16_t type;       // P2InputType
  uint16_t flags;      // P2_INPUT_FLAG_*
  int32_t  x, y;       // istemci-uzayı px (fare) ya da resize boyutu
  int32_t  wheelDelta;
  uint32_t key;        // Win32 VK (key) ya da UTF-16 birimi (char) ya da kısayol id (hotkey)
  uint32_t reserved;
};
#pragma pack(pop)

// Kirli dikdörtgen yardımcıları (iki taraf da kullanır).
// "Tam" sayılır: W ya da H <= 0, ya da dikdörtgen fw x fh karenin tamamını kaplıyor.
static inline int p2_rect_is_full(int32_t x, int32_t y, int32_t w, int32_t h, uint32_t fw, uint32_t fh) {
  if (w <= 0 || h <= 0) return 1;
  return x <= 0 && y <= 0 && (int64_t)x + w >= (int64_t)fw && (int64_t)y + h >= (int64_t)fh;
}
// Dikdörtgeni fw x fh kareye kırpar; boş kaldıysa 0 döner.
static inline int p2_rect_clip(int32_t* x, int32_t* y, int32_t* w, int32_t* h, uint32_t fw, uint32_t fh) {
  int64_t x0 = *x < 0 ? 0 : *x, y0 = *y < 0 ? 0 : *y;
  int64_t x1 = (int64_t)*x + *w, y1 = (int64_t)*y + *h;
  if (x1 > (int64_t)fw) x1 = fw;
  if (y1 > (int64_t)fh) y1 = fh;
  if (x1 <= x0 || y1 <= y0) { *w = *h = 0; return 0; }
  *x = (int32_t)x0; *y = (int32_t)y0; *w = (int32_t)(x1 - x0); *h = (int32_t)(y1 - y0);
  return 1;
}

static inline uint32_t p2_frame_slot_offset(uint32_t index) {
  return (uint32_t)sizeof(P2OverlayHeader) + index * (uint32_t)P2_OVL_SLOT_BYTES;
}
static inline uint32_t p2_shot_slot_offset() {
  return (uint32_t)sizeof(P2OverlayHeader) + 2u * (uint32_t)P2_OVL_SLOT_BYTES;
}
static inline uint32_t p2_total_mapping_bytes() {
  return (uint32_t)sizeof(P2OverlayHeader) + 2u * (uint32_t)P2_OVL_SLOT_BYTES + (uint32_t)P2_OVL_SHOT_BYTES;
}
