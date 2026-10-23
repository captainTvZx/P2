// Basit dosya günlüğü: %LocalAppData%\P2\overlay.log
#include "log.h"
#include <windows.h>
#include <shlobj.h>
#include <stdio.h>
#include <stdarg.h>

static void logPath(wchar_t* out, size_t n) {
  PWSTR base = nullptr;
  if (SUCCEEDED(SHGetKnownFolderPath(FOLDERID_LocalAppData, 0, nullptr, &base))) {
    swprintf(out, n, L"%s\\P2\\overlay.log", base);
    CoTaskMemFree(base);
  } else {
    wcsncpy_s(out, n, L"overlay.log", _TRUNCATE);
  }
}

void p2log(const char* fmt, ...) {
  wchar_t path[MAX_PATH];
  logPath(path, MAX_PATH);
  // P2 klasörünü oluştur (yoksa).
  wchar_t dir[MAX_PATH]; wcsncpy_s(dir, path, _TRUNCATE);
  wchar_t* slash = wcsrchr(dir, L'\\'); if (slash) { *slash = 0; CreateDirectoryW(dir, nullptr); }
  FILE* f = nullptr;
  if (_wfopen_s(&f, path, L"a") || !f) return;
  SYSTEMTIME st; GetLocalTime(&st);
  fprintf(f, "[%02d:%02d:%02d] [dll] ", st.wHour, st.wMinute, st.wSecond);
  va_list args; va_start(args, fmt); vfprintf(f, fmt, args); va_end(args);
  fprintf(f, "\n");
  fclose(f);
}
