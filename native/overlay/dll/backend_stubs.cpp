// Vulkan / OpenGL arka uçları — iskelet (D3D11 ve D3D12 gerçek; bkz. backend_d3d11/12.cpp).
// İlk sürümde D3D11 birincildir. Bu arka uçlar sonraki sprintlerde doldurulacak;
// şimdilik "desteklenmiyor" döner, böylece overlay bu API'leri kullanan oyunlarda
// sessizce devre dışı kalır (oyun normal çalışmaya devam eder).
// Ekran görüntüsü servisi de no-op: Present hook'u olmadığından istek hiç görülmez; host zaman
// aşımıyla desktopCapturer'a düşer. Gerçek arka uç geldiğinde shot::pending/fail/complete kullanılmalı.
#include "ipc_client.h"
#include "log.h"

namespace vkbackend    { bool install(IpcClient*) { p2log("vulkan arka ucu henuz yok"); return false; } void uninstall() {} void service() {} }
namespace glbackend    { bool install(IpcClient*) { p2log("opengl arka ucu henuz yok"); return false; } void uninstall() {} void service() {} }
