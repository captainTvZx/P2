@echo off
setlocal
rem P2 Server (Rust) gercek Windows hizmeti: WinSW, p2-server.exe --p2-server calistirir.
rem services.msc icinde gorunur ve bilgisayar
rem acilinca kullanici oturumu beklemeden baslar.
rem Yonetici izni gerekir; gerekiyorsa kendini yukseltir.
rem Not: parantezli bloklarin icinde "::" yorumu ve %DEGISKEN% genisletmesi
rem batch'te tuzaktir (blok ayristirilirken genisler); o yuzden SID blok disinda alinir.

rem Kurulumu yapan kullanicinin SID'i: yukseltilmis bolum baska bir hesapla calisabilir,
rem bilgisayar adi kullanici adiyla ayni olabilir; isim yerine SID kullanilir.
set "P2_USER_SID=%~1"
if "%P2_USER_SID%"=="" for /f "tokens=2 delims=," %%s in ('whoami /user /fo csv /nh') do set "P2_USER_SID=%%~s"

net session >nul 2>&1
if %errorlevel% neq 0 (
  echo Yonetici izni isteniyor...
  powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -ArgumentList '%P2_USER_SID%' -Verb RunAs"
  exit /b
)

set "P2_SERVICE=%~dp0P2ServerService.exe"
set "P2_DATA=%ProgramData%\P2"
set "P2_CONFIG_SCRIPT=%~dp0prepare-service-config.ps1"
if not exist "%P2_SERVICE%" (
  echo P2ServerService.exe bu klasorde bulunamadi: %~dp0
  pause
  exit /b 1
)
if not exist "%P2_DATA%" mkdir "%P2_DATA%"
if not exist "%P2_CONFIG_SCRIPT%" (
  echo Hizmet yapilandirma betigi bulunamadi: %P2_CONFIG_SCRIPT%
  pause
  exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%P2_CONFIG_SCRIPT%" -InstallDir "%~dp0"
if errorlevel 1 (
  echo Hizmet yapilandirmasi olusturulamadi.
  pause
  exit /b 1
)
rem 0.7.13 ve onceki surumlerin olusturdugu ayni adli Gorev Zamanlayici
rem kaydini temizle; aksi halde hizmetle birlikte ikinci bir sunucu acilabilir.
schtasks /Delete /TN "P2 Server" /F >nul 2>&1
rem Sunucu yapilandirmasi (veritabani parolasi) yalnizca yoneticiler, SYSTEM ve kurucu okuyabilsin.
icacls "%P2_DATA%" /inheritance:r /grant:r "*S-1-5-32-544:(OI)(CI)F" /grant:r "*S-1-5-18:(OI)(CI)F" >nul
if not "%P2_USER_SID%"=="" icacls "%P2_DATA%" /grant:r "*%P2_USER_SID%:(OI)(CI)F" >nul

sc query P2Server >nul 2>&1 && "%P2_SERVICE%" stop >nul 2>&1 && "%P2_SERVICE%" uninstall >nul 2>&1
"%P2_SERVICE%" install >nul 2>&1
if errorlevel 1 (
  echo Servis kaydedilemedi.
  pause
  exit /b 1
)
"%P2_SERVICE%" start >nul 2>&1
echo.
echo P2 Sunucu servisi kaydedildi ve baslatildi.
echo   - Hizmet adi: P2 Server (P2Server)
echo   - Bilgisayar her acildiginda kendiliginden calisir.
echo   - Kayit: %P2_DATA%\server.log
echo   - Kaldirmak icin: p2-server-remove.bat
echo.
timeout /t 3 >nul
endlocal
