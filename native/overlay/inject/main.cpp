// p2inject64.exe — oyunu askıya alınmış başlatır, p2overlay64.dll'i uzak iş
// parçacığıyla yükler, sonra devam ettirir. Enjeksiyon mantığı Electron'dan ayrı
// bir süreçte izole edilir (AV/EDR için daha temiz).
//
// Kullanım:
//   p2inject64.exe <dll-yolu> <exe-yolu> <cwd> [arg...]   — başlat + enjekte, pid'i yazar
//   p2inject64.exe --attach <dll-yolu> <pid>              — çalışan sürece enjekte
// İkinci mod Unreal gibi "kök exe sadece başlatıcı, asıl oyun alt süreç" düzenleri için:
// Electron tarafı alt süreçleri izler ve asıl oyun süreci gelince buraya verir.
#include <windows.h>
#include <stdio.h>
#include <string>
#include <vector>

static std::wstring quote(const std::wstring& s) { return L"\"" + s + L"\""; }

// Hedef sürece LoadLibraryW(dll) çalıştıran uzak iş parçacığı. 0 = başarı.
static int injectInto(HANDLE process, const std::wstring& dll) {
  SIZE_T bytes = (dll.size() + 1) * sizeof(wchar_t);
  LPVOID remote = VirtualAllocEx(process, nullptr, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  int code = 0;
  if (remote && WriteProcessMemory(process, remote, dll.c_str(), bytes, nullptr)) {
    HMODULE k32 = GetModuleHandleW(L"kernel32.dll");
    auto loadLib = (LPTHREAD_START_ROUTINE)GetProcAddress(k32, "LoadLibraryW");
    HANDLE th = CreateRemoteThread(process, nullptr, 0, loadLib, remote, 0, nullptr);
    if (th) {
      WaitForSingleObject(th, 10000);
      DWORD result = 0; GetExitCodeThread(th, &result);
      if (!result) { fwprintf(stderr, L"LoadLibraryW hedefte basarisiz\n"); code = 6; }
      CloseHandle(th);
    } else { fwprintf(stderr, L"CreateRemoteThread basarisiz: %lu\n", GetLastError()); code = 4; }
  } else { fwprintf(stderr, L"uzak bellek yazilamadi\n"); code = 5; }
  if (remote) VirtualFreeEx(process, remote, 0, MEM_RELEASE);
  return code;
}

static int attachMode(int argc, wchar_t** argv) {
  if (argc < 4) { fwprintf(stderr, L"kullanim: p2inject64 --attach <dll> <pid>\n"); return 2; }
  std::wstring dll = argv[2]; DWORD pid = (DWORD)wcstoul(argv[3], nullptr, 10);
  if (!pid) { fwprintf(stderr, L"gecersiz pid\n"); return 2; }
  HANDLE process = OpenProcess(PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ, FALSE, pid);
  if (!process) { fwprintf(stderr, L"OpenProcess basarisiz: %lu\n", GetLastError()); return 3; }
  // 32-bit sürece 64-bit DLL yüklenemez; sessizce atla.
  BOOL wow64 = FALSE; if (IsWow64Process(process, &wow64) && wow64) { CloseHandle(process); fwprintf(stderr, L"32-bit surec, atlandi\n"); return 7; }
  int code = injectInto(process, dll);
  CloseHandle(process);
  wprintf(L"%lu\n", pid);
  return code;
}

int wmain(int argc, wchar_t** argv) {
  if (argc >= 2 && wcscmp(argv[1], L"--attach") == 0) return attachMode(argc, argv);
  if (argc < 4) { fwprintf(stderr, L"kullanim: p2inject64 <dll> <exe> <cwd> [arg...]\n"); return 2; }
  std::wstring dll = argv[1], exe = argv[2], cwd = argv[3];
  std::wstring cmd = quote(exe);
  for (int i = 4; i < argc; i++) { cmd += L" "; cmd += argv[i]; }
  std::vector<wchar_t> cmdBuf(cmd.begin(), cmd.end()); cmdBuf.push_back(0);

  STARTUPINFOW si{ sizeof(si) }; PROCESS_INFORMATION pi{};
  if (!CreateProcessW(exe.c_str(), cmdBuf.data(), nullptr, nullptr, FALSE, CREATE_SUSPENDED, nullptr, cwd.c_str(), &si, &pi)) {
    fwprintf(stderr, L"CreateProcess basarisiz: %lu\n", GetLastError()); return 3;
  }
  int code = injectInto(pi.hProcess, dll);
  ResumeThread(pi.hThread);
  // pid'i stdout'a yaz ki Electron tarafı süreci izleyebilsin.
  wprintf(L"%lu\n", pi.dwProcessId);
  CloseHandle(pi.hThread); CloseHandle(pi.hProcess);
  return code;
}
