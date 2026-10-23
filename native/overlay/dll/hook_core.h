#pragma once
#include "ipc_client.h"

// Çizim API'sinin sunum (Present) çağrısına bağlanıp overlay karesini bindirir.
namespace hooks {
  // Kurulum: önce D3D11 denenir, sonra D3D12/Vulkan/OpenGL (stub). Başarısızsa false.
  bool install(IpcClient* ipc);
  void uninstall();
  // Her sunumda çağrılır: paylaşılan bellekten BGRA kareyi backbuffer üstüne çizer.
  // Backend dosyaları bu yardımcıları kullanır.
}
