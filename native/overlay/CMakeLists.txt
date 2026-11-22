# P2 oyun içi overlay — enjekte edilen DLL + enjektör (x64, MSVC).
#   cmake -S native/overlay -B native/overlay/build -A x64
#   cmake --build native/overlay/build --config Release
# Çıktılar: build/Release/p2overlay64.dll, build/Release/p2inject64.exe
cmake_minimum_required(VERSION 3.20)
project(p2overlay LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# MinHook: thirdparty/minhook altına klonlanmalı (bkz. README) ya da FetchContent.
include(FetchContent)
FetchContent_Declare(minhook GIT_REPOSITORY https://github.com/TsudaKageyu/minhook.git GIT_TAG master)
FetchContent_MakeAvailable(minhook)

add_library(p2overlay64 SHARED
  dll/dllmain.cpp
  dll/hook_core.cpp
  dll/backend_d3d11.cpp
  dll/backend_d3d12.cpp
  dll/shaders_d3d11.cpp
  dll/backend_stubs.cpp
  dll/shot.cpp
  dll/ipc_client.cpp
  dll/input_hooks.cpp
  dll/log.cpp
)
target_include_directories(p2overlay64 PRIVATE shared ${minhook_SOURCE_DIR}/include)
target_link_libraries(p2overlay64 PRIVATE minhook d3d11 d3d12 dxgi d3dcompiler shell32 ole32)
set_target_properties(p2overlay64 PROPERTIES OUTPUT_NAME "p2overlay64")
# Çökme dökümlerinde adres -> fonksiyon eşlemesi için harita dosyası.
target_link_options(p2overlay64 PRIVATE /MAP)

add_executable(p2inject64 inject/main.cpp)
target_include_directories(p2inject64 PRIVATE shared)
set_target_properties(p2inject64 PROPERTIES OUTPUT_NAME "p2inject64")
