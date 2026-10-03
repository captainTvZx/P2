@echo off
setlocal
rem P2 Sunucu servisini durdurur ve acilis kaydini kaldirir. Veritabanina dokunmaz.
net session >nul 2>&1
if %errorlevel% neq 0 (
  rem yonetici izni
  powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
  exit /b
)
set "P2_SERVICE=%~dp0P2ServerService.exe"
if exist "%P2_SERVICE%" (
  "%P2_SERVICE%" stop >nul 2>&1
  "%P2_SERVICE%" uninstall >nul 2>&1
) else (
  sc stop P2Server >nul 2>&1
  sc delete P2Server >nul 2>&1
)
echo P2 Sunucu servisi kaldirildi. Sunucuyu tekrar kurmak icin p2-server-install.bat calistir.
timeout /t 5 >nul
endlocal
