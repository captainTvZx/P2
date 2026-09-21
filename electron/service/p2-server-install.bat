@echo off
setlocal
:: P2 Sunucu servisi: bilgisayar acilinca (oturum acilmasa da) sessizce baslar.
:: Yonetici izni gerekir; gerekiyorsa kendini yukseltir.
net session >nul 2>&1
if %errorlevel% neq 0 (
  echo Yonetici izni isteniyor...
  powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
  exit /b
)

set "P2_EXE=%~dp0P2.exe"
set "P2_DATA=%ProgramData%\P2"
if not exist "%P2_EXE%" (
  echo P2.exe bu klasorde bulunamadi: %~dp0
  pause
  exit /b 1
)
if not exist "%P2_DATA%" mkdir "%P2_DATA%"
:: Sunucu yapilandirmasi (veritabani parolasi) yalnizca yoneticiler ve SYSTEM tarafindan okunabilsin.
icacls "%P2_DATA%" /inheritance:r /grant:r "*S-1-5-32-544:(OI)(CI)F" /grant:r "*S-1-5-18:(OI)(CI)F" /grant:r "%USERNAME%:(OI)(CI)F" >nul

schtasks /Query /TN "P2 Server" >nul 2>&1 && schtasks /End /TN "P2 Server" >nul 2>&1
schtasks /Create /F /SC ONSTART /RU SYSTEM /RL HIGHEST /TN "P2 Server" /TR "\"%P2_EXE%\" --p2-server" >nul
if %errorlevel% neq 0 (
  echo Servis kaydedilemedi.
  pause
  exit /b 1
)
schtasks /Run /TN "P2 Server" >nul
echo.
echo P2 Sunucu servisi kaydedildi ve baslatildi.
echo   - Bilgisayar her acildiginda kendiliginden calisir.
echo   - Kayit: %P2_DATA%\server.log
echo   - Kaldirmak icin: p2-server-remove.bat
echo.
timeout /t 6 >nul
endlocal
