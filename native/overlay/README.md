# P2 Oyun İçi Overlay — enjekte DLL + enjektör (C++)

Oyun sürecine **enjekte edilen** overlay. Kısayolla (varsayılan Shift+2) açılır; sesli oda, sohbet,
ekip durumu, görev/not araçları, performans, HUD ve ekran görüntüsü sunar. Overlay arayüzü Rust
overlay host'u (`apps/p2-overlay`, oyun süreci başına bir `p2-overlay.exe`) tarafından GPUI ile
**ekran dışında** çizilir; bu klasördeki DLL kareyi oyunun çizdiği karenin üstüne bindirir.

## Parçalar

| Yol | İçerik |
|---|---|
| `shared/p2overlay_ipc.h` | Paylaşımlı bellek + girdi borusu sözleşmesi (şema 4). Rust karşılığı: `crates/p2-overlay-ipc` |
| `dll/` | Enjekte DLL (`p2overlay64.dll`): Present/ExecuteCommandLists kancaları, **D3D11 ve D3D12 tam**, Vulkan/OpenGL iskelet; girdi kancaları, ekran görüntüsü |
| `inject/` | Enjektör (`p2inject64.exe`): oyunu askıya alınmış başlatır ya da çalışan sürece bağlanır (`--attach`) |

Rust tarafı: `crates/p2-overlay-client` (istemcide; enjektörle başlatma, alt süreç izleme, host süreçleri),
`apps/p2-overlay` (host), `apps/p2-overlay-host` + `apps/p2-d3dtest` (geliştirme/test aracı).

**Güvenli devre dışı:** `p2inject64.exe` / `p2overlay64.dll` bulunamazsa istemci oyunu normal
(enjeksiyonsuz) başlatır.

## Derleme

Önkoşul: Visual Studio 2022+ (C++ masaüstü iş yükü). CMake PATH'te değilse VS'nin kendi CMake'i bulunur
(`P2_CMAKE` ile de verilebilir). MinHook, CMake `FetchContent` ile indirilir. Node yalnız betiği
çalıştırmak için gerekir; npm paketi yok.

```powershell
node packaging/build-overlay.mjs            # yapılandır + derle + native/overlay/dist'e topla
node packaging/build-overlay.mjs --collect  # yalnız var olan derlemeyi dist'e topla
# ya da elle:
cmake -S native/overlay -B native/overlay/build -A x64
cmake --build native/overlay/build --config Release
```

Çıktılar: `native/overlay/build/Release/{p2overlay64.dll, p2inject64.exe}` → `native/overlay/dist/`.
`dist` ve `build` git'e girmez. `packaging/build-installers.mjs`, `dist` eksikse derlemeyi kendisi
başlatır (`--rebuild-overlay` her seferinde yeniden derler) ve iki dosyayı istemci paketine
`overlay\` altına koyar.

Geliştirme döngüsü: C++ düzenle → `node packaging/build-overlay.mjs` → oyunu Kütüphane'den yeniden
çalıştır (DLL her başlatmada taze yüklenir). Geliştirmede istemci ikilileri `native/overlay/dist`'ten
okur (`P2_OVERLAY_DIR` ile değiştirilebilir); kurulu pakette `<exe klasörü>\overlay`.
`shared/p2overlay_ipc.h` değişince **hem DLL hem `crates/p2-overlay-ipc`** güncellenir (ofsetler
`offsetof` ile birebir; Rust tarafında düzen testleri var).

## Süreç ağacı (Unreal başlatıcısı)

Unreal'ın paketlenmiş oyunlarında kök `Oyun.exe` yalnız bir başlatıcıdır (~170 KB,
BootstrapPackagedGame); asıl oyun `Oyun/Binaries/Win64/Oyun.exe` **alt süreci**dir.
Bu yüzden istemci (`crates/p2-overlay-client`) kök pid'i izler, saniyede bir Toolhelp anlık
görüntüsüyle alt ağacı tarar ve her yeni 64-bit alt sürece `p2inject64 --attach <dll> <pid>` ile de
enjekte eder (CrashReportClient, CEF alt süreci, prereq kurucu gibi yardımcılar atlanır). Hangi sürecin
oyun olduğunu DLL'in Present sayacı (`header.frames`, şema 3+) söyler: ilk kareyi çizen süreç "hazır"dır.
Teşhis: `overlay.log` — `[dll]` satırlarında exe adı, `[host]` satırlarında hangi pid'e enjekte edildiği
ve ilk kare bilgisi.

> Aşağıdaki IPC ayrıntıları DLL ile host arasındaki sözleşmedir ve geçerlidir. "Host" bugün Rust overlay
> host'udur (`apps/p2-overlay` + `crates/p2-overlay-ipc`); Electron/`overlayHost.cjs`/`host.node`
> geçen notlar, sözleşmenin ilk uygulandığı Electron dönemine ait tarihsel kayıttır.

## IPC sözleşmesi — şema 4 (`shared/p2overlay_ipc.h`)

Başlık + 2 kare yuvası (`P2_OVL_SLOT_BYTES`; kısmi kare yolunda yalnız **yuva 0** kullanılır, bkz. aşağı) + 1 ekran
görüntüsü yuvası (`P2_OVL_SHOT_BYTES = 2×slot`, 4K RGBA16F için). İki taraf ofsetleri `sizeof(P2OverlayHeader)` ile
hesaplar: başlık değişince **DLL ve host birlikte** derlenir; şema uyuşmazsa DLL bağlanmaz (`ipc_client.cpp`), overlay
sessizce kapalı kalır.

Şema 4 alanları:

| Alan | Yazan | Anlam |
| --- | --- | --- |
| `visible` | DLL (kısayol) **ve** host (`setVisible`) | **Tek doğruluk kaynağı.** Worker döngüsü artık yerel durumu başlığa yazmaz; başlık değişince `input::setOverlayVisible` ile imleç/ClipCursor uygulanır. Host `visible=0` yazınca DLL bir tik (≤16 ms) içinde imleci bırakır. Log: `gorunurluk: host kapatti/acti` (host kaynaklı), `gorunurluk: ACIK/kapali` (kısayol). |
| `drawMask` | host (`setDrawMask`; `setNotify` = bit0 takma adı) | Eski `notify` alanı, aynı ofset. bit0 TOAST, bit1 PINNED, bit2 HUD, bit3 DEBUG. DLL `visible || drawMask` iken çizer; `drawMask` tek başına girdi yakalamaz. |
| `hotkeyGen`, `hotkeys[8]` | host (`create`, `setHotkeys`; mutex altında, gen++) | `{vk, mods, id}`; `mods` bit0 shift, bit1 ctrl, bit2 alt, **bit7 pass-through** (tuş oyuna da gider; bas-konuş için). id: 1 panel, 2 ekran görüntüsü, 3 sus, 4 bas-konuş, 5 sağırlaştır. DLL worker'ı her tik `hotkeyGen`'i karşılaştırır, değişince tabloyu mutex altında kopyalar (`kisayol tablosu yuklendi (gen=N, n adet)`). Tablo boşsa `hotkeyVk/hotkeyMods` id 1 olarak kullanılır (geriye uyum). |
| `hotkeyVk/Mods` | host | id 1'in aynası (eski DLL'ler ve `create(pid, vk, mods, cb)` imzası için). |
| `shotRequestId` | host (`requestShot`, mutex) | Artınca DLL bir sonraki Present'te arka tamponu (overlay çizilmeden önce) GPU ara kaynağına kopyalar, sonraki karelerde yuvaya okur (bkz. "Ekran görüntüsü"). |
| `shotReadyId`, `shotW/H/Stride/Format`, `shotFlags` | DLL | Kopya bitince `shotReadyId = shotRequestId`, boyut + ham DXGI formatı, `shotFlags` bit0 = başarısız; ardından `P2_INPUT_SHOT_READY` mesajı. Host `readShot` yalnız `shotReadyId == shotRequestId && !(shotFlags & 1)` iken yuvayı sıkı BGRA8'e çevirir (87/91 BGRA8, 28/29 RGBA8, 24 R10G10B10A2, 10 RGBA16F). |
| `inputFocus` | DLL (`WM_ACTIVATE`) | 1 oyun penceresi etkin / 0 değil (FOCUS mesajına ek; host anlık okur). |
| `frames` | DLL | Present sayacı (ilk kare = oyun çizmeye başladı). |
| `width/height/stride`, `frontIndex`, `frameId`, `dirtyX/Y/W/H` | host (`writeFrame`, mutex) | Kare: `frontIndex` hep 0 (tek kalıcı yuva). `dirty` = DLL'in son ack'inden beri yazılan dikdörtgenlerin **birleşimi** (W/H 0 ya da tam kaplama = tam kare). `frameId` her yazımda +1. |
| `reserved2[0]` (`P2_OVL_ACK_FRAME_SLOT`) | DLL (mutex, kopyayla birlikte) | DLL'in GPU'ya yüklediği son `frameId` (düşük 32 bit). Host bunu `frameId` ile karşılaştırıp dikdörtgeni sıfırdan mı başlatacağına, birleştirecek mi karar verir. |
| `reserved2[1]` (`P2_OVL_CURSOR_SLOT`) | host (`setCursor`) | İmleç şekli: Chromium `cursor-changed` türü Win32 IDC_* kaynak kimliğine çevrilir (32512 ok, 32513 I-beam, 32642–32645 çift oklar, 32649 el). DLL overlay açıkken `SetCursor`/`WM_SETCURSOR`'da bu şekli kullanır; değişince hemen uygular. 0 = ok. |

Girdi borusu (DLL → host, sabit 24 baytlık `P2InputMsg`): `MOUSE_MOVE/BTN/WHEEL`, `KEY`, `CHAR`, `TOGGLE`, `FOCUS`,
`RESIZE`, **`HOTKEY=9`** (`key`=id, `flags` bit0 down, `x`=vk, `y`=mods), **`SHOT_READY=10`** (`key`=requestId,
`x`=w, `y`=h, `wheelDelta`=format, `flags` bit0 başarısız). Host her mesajı `{type, flags, x, y, wheelDelta, key}`
olarak `onInput`'a verir.

### Kısayol değerlendirmesi (DLL, `input_hooks.cpp`)

- `WM_(SYS)KEYDOWN` (otomatik tekrar `lParam & (1<<30)` yok sayılır) ve `WM_(SYS)KEYUP`, overlay açık **ya da kapalı**,
  `if (g_visible)` yakalama bloğundan önce değerlendirilir.
- Eşleşme: `vk` + **tam** modifier kümesi (`GetKeyState` shift/ctrl/alt). Shift+2, Ctrl+Shift+2 ile eşleşmez. Kısayolun
  kendisi modifier ise (bas-konuş = Sol Shift) o bit sayılmaz.
- id 1: yerel panel toggle (başlık + `TOGGLE` mesajı, flags bit0 = yeni görünürlük), tuş yutulur.
- Diğer id'ler: `HOTKEY` mesajı; pass-through değilse tuş yutulur. Yalnız bas-konuş (id 4) `up` da gönderir.
  Basılı tuşun kısayol id'si vk ile hatırlanır: Shift önce bırakılsa bile `up` üretilir (mikrofon açık kalmaz);
  odak kaybında kayıt sıfırlanır (`WM_ACTIVATE`).
- Log: `hotkey id=N down/up`.

### Girdi kalitesi

- Tekerlek: `WM_MOUSEWHEEL/WM_MOUSEHWHEEL` ekran koordinatı `ScreenToClient` ile istemciye çevrilip `x,y` gider; yatay
  tekerlek `P2_INPUT_FLAG_HWHEEL` (bit8).
- `KEY` down'da otomatik tekrar `P2_INPUT_FLAG_REPEAT` (bit8).
- `WM_CHAR` UTF-16 birimi aynen gider (surrogate çiftlerini host birleştirir); `WM_DEADCHAR/WM_SYSCHAR/WM_SYSDEADCHAR`
  görünürken yutulur. IME kapsam dışı.

### Eski Node host eklentisi dışa aktarımları (`host/host.cc`, kaldırıldı)

Tarihsel kayıt: Electron sürümünün N-API eklentisi. Rust karşılığı `crates/p2-overlay-ipc` (paylaşımlı bellek,
kısmi kare, girdi borusu) ve `crates/p2-overlay-client` (süreç ağacı, enjeksiyon); aynı sözleşme.


`create(pid, {hotkeys:[{id,vk,mods,passThrough}]}, onInput)` **veya** eski `create(pid, vk, mods, onInput)` ·
`setHotkeys(pid, list)` · `setCursor(pid, idcId)` · `setDrawMask(pid, mask)` · `setNotify(pid, bool)` (bit0 takma adı) · `setVisible(pid, bool)` ·
`visible(pid)` · `frames(pid)` · `writeFrame(pid, buf, fullW, fullH, dx, dy, dw, dh)` (tam ya da kısmi; bkz. "Kısmi kare
yolu") · `requestShot(pid) -> id` · `readShot(pid) -> {id,width,height,buffer}|null` · `stats(pid) -> {frames, visible,
drawMask, hotkeyGen, shotRequestId, shotReadyId, shotFlags, width, height, inputFocus, frameId, ackFrameId,
dirty:{x,y,w,h}}|null` · `windowTitle(pid) -> string` (pid'in en büyük görünür üst penceresi; `desktopCapturer` yedeği) ·
`processStats(pid) -> {cpuMs, wallMs, ramMb, cores}|null` (oyun sürecinin toplam CPU süresi + çalışma belleği; host iki örnek farkından "Oyun CPU %" hesaplar, Performans penceresi/HUD gösterir) ·
`processes()` · `destroy(pid)` · `schema` (sabit, 4) · `partialFrames` (sabit, `true`: eklenti kısmi kare kabul eder;
eski eklentilerde `undefined`).

## Kısmi kare yolu (kirli dikdörtgen, `writeFrame`)

Sorun: her Chromium boyamasında tam kare üç kez kopyalanıyordu (`toBitmap()` → paylaşılan bellek → DLL doku); 2560×1440'ta
~15 MB × 3, pencere sürüklerken saniyede 30 boyama → takılma. Şimdi uçtan uca yalnız **kirli dikdörtgen** taşınır.

- **Tek kalıcı yuva.** Host çift tampon kullanmaz: yuva 0 her zaman tam ve güncel kareyi taşır, `frontIndex` 0 kalır.
  Her yazım (tam ya da dikdörtgen) **başlık mutex'i altında** yapılır; DLL de dikdörtgeni GPU'ya kopyalarken mutex'i
  tutar (8 ms zaman aşımı; alınamazsa o Present atlanır, `g_lastFrame` ilerlemez, sonraki Present'te yeniden denenir).
  Kopyalar küçük olduğu için çekişme ihmal edilir; tam kare yazımı (ilk kare, boyut değişimi, tüm pencereyi kaplayan
  boyama) birkaç ms sürer ve DLL o sırada bekler.
- **`writeFrame(pid, buffer, fullW, fullH, dx, dy, dw, dh)` sözleşmesi:**
  - `buffer.length >= fullW*fullH*4` → **tam kare**: yuvanın tamamı kopyalanır (boyut değişince de bu yol; `fullW/fullH`
    yeni boyut olur). `dirty = (0,0,fullW,fullH)`.
  - aksi hâlde **kısmi**: `buffer.length` tam olarak `dw*dh*4` olmalı, `fullW/fullH` başlıktaki boyutla aynı olmalı ve
    yuvada bir tam kare bulunmalı (`frameId > 0`); satır satır `(dx,dy)`'ye kopyalanır, dikdörtgen kareye kırpılır
    (negatif/taşan kenarlar kaynakta da ofsetlenir). Şu durumlarda `false` döner ve `frameId` artmaz: tam kare yokken
    kısmi, boyut uyuşmazlığı, yanlış uzunluk, `dw/dh <= 0`, tamamen kare dışında dikdörtgen. Electron tarafı `false`
    görünce bir sonraki boyamada tam kare göndermelidir.
  - Her başarılı yazımda: `width/height/stride`, `dirty`, `frameId++`, frame-ready olayı.
- **Ack ve birleşim.** İki Present arasında birden çok boyama gelirse DLL yalnız son başlığı görür; kaçırdığı
  dikdörtgenleri de yüklemesi gerekir. DLL yüklediği `frameId`'nin düşük 32 bitini `reserved2[P2_OVL_ACK_FRAME_SLOT]`'a
  (mutex altında, kopyayla aynı kilitte) yazar. Host yazmadan önce `ack == (uint32_t)frameId` ise DLL güncel → `dirty`
  yeni dikdörtgen; değilse `dirty = union(önceki dirty, yeni dikdörtgen)` (önceki tamsa tam kalır). DLL çizmiyorken
  (`visible=0`, `drawMask=0`) ack ilerlemez, birleşim en çok tam kareye büyür; çizim başlayınca tek seferde yüklenir.
  Başlık düzeni değişmedi (şema 4).
- **DLL D3D11** (`backend_d3d11.cpp`): doku `D3D11_USAGE_DEFAULT` (CPU erişimi yok). `frameId` değişince mutex altında
  başlık okunur; dikdörtgen boş/tam ya da doku yeni oluşturulduysa (boyut değişimi) `UpdateSubresource` tam, yoksa
  `D3D11_BOX{dx,dy,dx+dw,dy+dh}` ile yalnız dikdörtgen (`pSrcData = yuva + dy*stride + dx*4`, `SrcRowPitch = stride`).
  Çizim yolu (premultiplied `ONE/INV_SRC_ALPHA`) aynı.
- **DLL D3D12** (`backend_d3d12.cpp`): yalnız dikdörtgen satırları yuvanın upload tamponuna (`pitch = (dw*4+255)&~255`,
  tam kare için boyutlanmış tampona her zaman sığar) kopyalanır; `CopyTextureRegion(dst{x:dx,y:dy}, PLACED_FOOTPRINT{BGRA8,
  dw, dh, 1, pitch}, box{0,0,dw,dh})`. Tam kopya: dikdörtgen tam ya da doku yeni (`g_texHasFrame=false`). Bariyerler
  aynı (`PIXEL_SHADER_RESOURCE ↔ COPY_DEST`).
- **Log:** ilk kısmi yüklemede `kismi kare yolu aktif (d3d11 UpdateSubresource box | d3d12 CopyTextureRegion, ilk
  dikdortgen WxH @x,y)`; `ilk arayuz karesi alindi`, `cizim basliyor`, `d3d12: ilk cizim` korunur.
- **Electron tarafı (`overlayHost.cjs`):** `addon.partialFrames === true` ise `paint` olayında `dirty` tüm kareyi
  kaplamıyorsa `image.crop(dirty).toBitmap()` ile `writeFrame(pid, bmp, size.width, size.height, dirty.x, dirty.y,
  dirty.width, dirty.height)`; kaplıyorsa ya da boyut değiştiyse `image.toBitmap()` ile tam kare (aynı imza).
  `crop(...).toBitmap()` **sıkı** `dw*dh*4` bayt döndürmeli (Electron NativeImage öyle yapar). `writeFrame` `false`
  dönerse (ör. ilk boyama kısmi geldi, ya da boyut değişti) bir sonraki boyamada tam kare gönder; en basit: `false`
  görülünce `win.webContents.invalidate()`. Eski eklentiyle (`partialFrames` yok) her zaman tam kare gönder.
- **Test (oyunsuz):** `partial-e2e.cjs` (scratchpad `shot-e2e/`): tam kare → 40×30 dikdörtgen → ardışık iki dikdörtgen
  (birleşim) → hatalı yazımlar (`false`) → taşan/negatif dikdörtgen (kırpma) → boyut değişimi (doku yeniden) → ekran
  görüntüsü (overlay dışarıda) → 200 hızlı kısmi yazım; her adımda `stats().ackFrameId` DLL'in yüklediğini doğrular.
  Doğrulandı (2026-10-23): D3D11 (discard, flip-model), D3D12; `shot-e2e.cjs` regresyonu D3D11 (msaa/rgba/srgb/flip)
  ve D3D12 geçer. Ölçüm (300×160 test exe): kısmi 200×120 yazımı ~3 µs, tam ~5 µs (host tarafı; asıl kazanım
  Electron'un `toBitmap()` ve DLL'in doku yüklemesinin dikdörtgene inmesi).

## Ekran görüntüsü (Faz 4, `dll/shot.h/.cpp` + arka uçlar)

Akış: host `requestShot(pid)` (mutex altında `shotFlags = 0`, `shotRequestId++`) → DLL Present hook'u → host
`readShot(pid)` (yalnız `shotReadyId == shotRequestId && !(shotFlags & 1)` iken sıkı BGRA8 döner). İstek, overlay
**çizilmese bile** servis edilir: `backend_d3d11.cpp::service(swap)` her Present'te önce `shotTick`, sonra
`(visible || drawMask) && kare var` ise çizim yapar; swapchain D3D12 ise `d3d12backend::service(swap, drawOverlay)`
ikisini birlikte yürütür. Tetik: `shotRequestId != shotReadyId && shotRequestId != g_shotInFlight` (`shot::pending`).
Kopya her zaman overlay çiziminden **önce** kaydedilir; görüntüde panel/toast yoktur.

İki aşama, GPU beklenmez:

- **D3D11** — kare N: `GetBuffer(0)` → staging doku (`D3D11_USAGE_STAGING`, `CPU_ACCESS_READ`, arka tamponla aynı
  format/boyut; boyut/format/cihaz aynıyken yeniden kullanılır) → `CopyResource`; arka tampon MSAA ise önce DEFAULT
  dokuya `ResolveSubresource`. Faz `COPIED`, log `shot: istek N kopyalandi (fmt=F WxH)`. Kare N+1..N+5:
  `Map(staging, D3D11_MAP_READ, DO_NOT_WAIT)`; `DXGI_ERROR_WAS_STILL_DRAWING` → sonraki kare (5 denemeden sonra
  başarısız). Başarıda satırlar sıkı `stride = w*bpp` ile yuvaya kopyalanır, mutex altında `shotW/H/Stride/Format`
  (ham `DXGI_FORMAT`), `shotFlags = 0`, `shotReadyId = id` yazılır, `SHOT_READY` gider, `Unmap`. Log `shot: hazir N`.
- **D3D12** — istek geldiğinde overlay'in kendi komut listesine (aynı 3 yuvalı halka; overlay çizilmiyorsa yalnız
  kopya içeren liste) çizimden önce: `PRESENT → COPY_SOURCE` bariyeri, `CopyTextureRegion(READBACK tamponu,
  PLACED_FOOTPRINT{format, w, h, 1, alignedPitch=(w*bpp+255)&~255}, arka tampon sub 0)`, bariyer geri
  (`RENDER_TARGET` bu karede çizim varsa, yoksa `PRESENT`). Yuvanın fence değeri `g_shotFence` olarak tutulur.
  Sonraki Present'lerde `fence->GetCompletedValue() >= g_shotFence` olunca `Map(readback, {0,size})`, satırlar
  `alignedPitch → sıkı` yuvaya, `Unmap(0, {0,0})`, başlık + `SHOT_READY`. READBACK tamponu boyut/format aynıyken yeniden
  kullanılır; değişince önce `waitAll` (GPU hâlâ yazıyor olabilir). 60 Present'te fence tamamlanmazsa başarısız.

Başarısız (`shotFlags |= 1`, `shotReadyId = id`, `SHOT_READY` flags bit0; host `desktopCapturer`'a düşer; log
`shot: basarisiz (neden)`): desteklenmeyen format (87/91 BGRA8, 28/29 RGBA8, 24 R10G10B10A2, 10 RGBA16F dışı),
`w*h*bpp > P2_OVL_SHOT_BYTES` ya da 3840×2160 üstü, D3D12'de henüz komut kuyruğu yakalanmamış (`komut kuyrugu yok`;
oyunun ilk `ExecuteCommandLists`'inden önce ya da kuyruk hook'u kurulurken), D3D12 arka ucu devre dışı (`g_failed`),
MSAA D3D12 arka tampon (flip-model'de olmaz), staging/readback oluşturulamadı, `Map` hatası, `DO_NOT_WAIT` 5 kare
dolu, komut listesi kapatılamadı. İstek sürerken yeni `requestShot` gelirse eski istek bırakılır (başlık yalnız
`shotRequestId == id` ise yazılır; log `shot: istek N gecersiz (yeni istek M)`), yenisi baştan işlenir. Vulkan/OpenGL:
Present hook'u olmadığından istek görülmez; host zaman aşımıyla düşer.

Elle test (oyunsuz): `cl` ile arka tamponu sabit renge temizleyip `Present` eden küçük bir D3D11/D3D12 exe derle
(300×160, `ClearRenderTargetView(0.2, 0.4, 0.8)` → BGRA8 `204,102,51`), `ELECTRON_RUN_AS_NODE=1 electron.exe
<harness.cjs>` ile `addon.create(pid, {hotkeys: []}, onInput)` → `p2inject64 --attach dist\p2overlay64.dll <pid>` →
`stats(pid).frames > 0` bekle (D3D12'de kuyruk hook'u için ~2 s daha) → `requestShot(pid)` → 50 ms aralıkla
`readShot(pid)`; `{id, width, height, buffer}` orta/köşe pikselleri beklenen renk ve `onInput` `type=10, key=id,
flags=0` olmalı. `writeFrame` ile opak kırmızı kare + `setDrawMask(pid, 1)` sonrası aynı istek yine temizleme rengini
vermeli (overlay dışarıda). Ardışık iki `requestShot` → `readShot.id` ikincisi. Doğrulandı (2026-10-23): D3D11 BGRA8
(+MSAA 4x, +flip-model), RGBA8, BGRA8_SRGB, R10G10B10A2, RGBA16F; D3D12 BGRA8, RGBA8 (overlay çizilirken ve çizilmezken).
`%LocalAppData%\P2\overlay.log` satırları: `shot: istek N kopyalandi (fmt=F WxH)`, `shot: hazir N`,
`shot: basarisiz (neden)`.

## D3D12 (UE5 varsayılanı)

DXGI Present hook'u D3D11 ve D3D12 swapchain'leri için ortaktır (aynı dxgi.dll vtable'ı).
Swapchain'in cihazı `ID3D11Device` değilse `backend_d3d12.cpp` çizer: oyunun DIRECT komut
kuyruğu `ID3D12CommandQueue::ExecuteCommandLists` hook'uyla yakalanır; her yeni karede paylaşılan
bellekteki kirli dikdörtgen upload tamponuna kopyalanıp dokunun o bölgesine aktarılır ve tam ekran üçgenle alfa
karıştırılarak backbuffer'a çizilir (3 yuvalı allocator/upload/fence halkası; bkz. "Kısmi kare yolu"). Overlay yüzeyi backbuffer boyutuna
eşitlenir (`RESIZE` mesajı → host `setSize`), fare koordinatları 1:1 örtüşür. Overlay açıkken
`WM_INPUT` (raw input) oyuna geçmez ve `SetCursorPos` no-op'tur (UE imleci ortalayamaz).

## Teşhis ve yerel test (0.9.8'de öğrenilenler)

- **Overlay penceresi `sandbox: false` olmalı.** `overlay-ui-preload.cjs` ana `preload.cjs`'i `require` eder;
  sandbox'lı preload dosya require edemez ("module not found: ./preload.cjs"), `window.p2overlay` köprüsü
  hiç kurulmaz ve arayüz ne bildirimi ne de görünürlük mesajını alır → Shift+2'de yalnız fare imleci çıkar,
  panel çizilmez. `contextIsolation` açık, `nodeIntegration` kapalı kalır.
- **Host boru okuyucusu olayları heap'te kopyalar.** `BlockingCall` asenkron teslim eder; yığın kopyası bir
  sonraki `ReadFile`'da ezildiği için tüm olaylar son mesajın içeriğiyle geliyordu (fare hareketi "toggle" olarak
  görünüyor, gerçek toggle kayboluyordu).
- **Premultiplied alfa.** Electron'un offscreen karesi premultiplied BGRA; blend `ONE / INV_SRC_ALPHA`.
  `SRC_ALPHA` ile panel olduğundan çok daha saydam görünür.
- **Arayüz yüklenmeden gelen görünürlük** `did-finish-load`'da yeniden gönderilir (`addon.visible(pid)`).
- Log satırları: DLL `ilk arayuz karesi alindi`, `cizim basliyor (visible/drawMask)`, `d3d12: ilk cizim`, `kismi kare yolu aktif`,
  `kisayol: overlay ACIK/kapali`, `gorunurluk: host kapatti/acti`, `kisayol tablosu yuklendi (gen=N, n adet)`,
  `hotkey id=N down/up`, `shot: istek N kopyalandi / hazir N / basarisiz (neden)`; host `overlay arayüzü yüklendi`, `arayüz karesi … yazıldı`,
  `overlay açıldı/kapandı (kısayol)`, arayüz konsol hataları.
- `P2_OVERLAY_DUMP=<klasör>`: host her 30. arayüz karesini PNG yazar (arayüzün gerçekten ne çizdiğini görmek için).
- Uygulama arayüzüne dokunmadan test: `electron <harness.cjs>` ile `overlayHost.configure({loadRenderer, suggestedServer})`
  + `startForGame(...)` doğrudan çağrılabilir (Vite 5173 açıkken `loadURL('http://localhost:5173/?overlay=1')`);
  `app.setAppPath(<repo>)` ile ikililer `native/overlay/build/Release`'ten bulunur. PowerShell `SendKeys('+2')`
  oyun penceresine kısayolu yollar; `CopyFromScreen` ile ekran görüntüsü alınabilir (borderless pencerede çalışır).
- Vite izleyicisi `native/**` ve derleme klasörlerini yok sayar (derleme sırasında EBUSY ile çöküyordu).

## Bilinen sınırlar (dürüst)

- **Anti-cheat:** EAC/BattlEye/Vanguard'lı build'lerde enjeksiyon yapılmaz
  (`overlayHost.hasAntiCheat` tespiti); overlay devre dışı, oyun normal çalışır.
  Bypass denenmez.
- **İmzasız:** Karar gereği DLL + enjektör imzasız. AV/SmartScreen bazı makinelerde
  enjeksiyonu işaretleyebilir; elle istisna gerekebilir.
- **Vulkan/OpenGL:** şimdilik "desteklenmiyor" döndüren iskelet; bu API'leri
  kullanan oyunlarda overlay sessizce kapalı kalır.
- **Ham klavye (RIDEV_NOLEGACY):** Kısayollar WndProc'taki `WM_KEYDOWN/KEYUP` ile yakalanır. Klavyeyi yalnız
  `RegisterRawInputDevices(RIDEV_NOLEGACY)` ile okuyan oyunlarda bu mesajlar hiç gelmez; orada Shift+2 dahil
  hiçbir kısayol çalışmaz (overlay açıkken `WM_INPUT` yine maskelenir). Çözüm (ileride): `WM_INPUT` içinden
  `GetRawInputData` ile klavye olaylarını da süzmek.
- **Modifier eşleşmesi tam:** Shift+2 tanımlıyken Ctrl+Shift+2 eşleşmez (bilinçli). Kısayol tablosu en çok 8 giriş.
- **Ekran görüntüsü:** yalnız D3D11/D3D12; Vulkan/OpenGL'de istek görülmez (host zaman aşımıyla `desktopCapturer`).
  D3D12'de kuyruk hook'u kurulmadan gelen istek hemen başarısız bayraklanır (oyun açılışındaki ilk ~1 s).
  RGBA16F (HDR) kaynak ham kopyalanır; host 0..1 kırpar, ton eşleme yok.
- **Offscreen kare biçimi:** Electron `paint` BGRA verir; DLL BGRA bekler.
  HDR (RGBA16F) swapchain'ler v1'de SDR'ye düşürülmeden ele alınmaz.
