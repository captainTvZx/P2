// Hook çekirdeği: MinHook'u başlatır ve uygun grafik arka ucunu seçer.
#include "hook_core.h"
#include "log.h"
#include "MinHook.h"

namespace d3d11backend { bool install(IpcClient*); void uninstall(); }
namespace d3d12backend { bool install(IpcClient*); void uninstall(); }
namespace vkbackend    { bool install(IpcClient*); void uninstall(); }
namespace glbackend    { bool install(IpcClient*); void uninstall(); }

namespace {
  int g_active = 0; // 11,12,0(vk),1(gl)
}

namespace hooks {
  bool install(IpcClient* ipc) {
    if (MH_Initialize() != MH_OK) { p2log("MinHook baslatilamadi"); return false; }
    // DXGI Present hook'u (d3d11backend) D3D11 ve D3D12 swapchain'leri için ortaktır; D3D12 çizimi
    // için ayrıca komut kuyruğu hook'u kurulur. Oyun RHI'yi enjeksiyondan sonra yüklediğinden
    // modül varlığına bakılmaz; her ikisi de geçici cihazla kurulur.
    if (d3d11backend::install(ipc)) {
      g_active = 11;
      if (!d3d12backend::install(ipc)) p2log("d3d12 hook kurulamadi; yalniz d3d11 oyunlarinda cizilir");
      return true;
    }
    if (GetModuleHandleW(L"vulkan-1.dll") && vkbackend::install(ipc)) { g_active = 2; return true; }
    if (GetModuleHandleW(L"opengl32.dll") && glbackend::install(ipc)) { g_active = 1; return true; }
    p2log("desteklenen grafik API bulunamadi; overlay devre disi");
    MH_Uninitialize();
    return false;
  }
  void uninstall() {
    if (g_active == 11) { d3d12backend::uninstall(); d3d11backend::uninstall(); }
    else if (g_active == 2) vkbackend::uninstall();
    else if (g_active == 1) glbackend::uninstall();
    MH_Uninitialize();
    g_active = 0;
  }
}
