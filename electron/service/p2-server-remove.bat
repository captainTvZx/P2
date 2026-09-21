@echo off
setlocal
:: P2 Sunucu servisini durdurur ve acilis kaydini kaldirir. Veritabanina dokunmaz.
net session >nul 2>&1
if %errorlevel% neq 0 (
  echo Yonetici izni isteniyor...
  powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
  exit /b
)
schtasks /End /TN "P2 Server" >nul 2>&1
schtasks /Delete /F /TN "P2 Server" >nul 2>&1
echo P2 Sunucu servisi kaldirildi. Sunucuyu tekrar kurmak icin p2-server-install.bat calistir.
timeout /t 5 >nul
endlocal
