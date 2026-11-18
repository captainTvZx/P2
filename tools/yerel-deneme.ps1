# Yerel deneme: test PostgreSQL + yerel Rust sunucusu (4010) + ayrı profilli P2 istemcisi.
# Canlı sunucuya ve gerçek profilinize (%APPDATA%\P2-Rust) dokunmaz.
#   powershell -ExecutionPolicy Bypass -File tools\yerel-deneme.ps1        → 1. oturum
#   powershell -ExecutionPolicy Bypass -File tools\yerel-deneme.ps1 2      → 2. oturum (ayrı profil, aynı sunucu)
# Test hesapları: p2test / Parola123 (sunucu sahibi), rustbot / Parola123, rustbot2 / Parola123. Çıkış: istemciyi kapatın; sunucu açık kalır
# (kapatmak için: Stop-Process -Name srv-deneme).
param([string]$Oturum = '1')
$ErrorActionPreference = 'Stop'
# Hata olursa pencere kapanmadan önce mesaj gösterilir.
trap { Write-Host "HATA: $($_.Exception.Message)" -ForegroundColor Red; Read-Host 'Kapatmak için Enter'; exit 1 }
$pg = 'C:\p2-pg'
$bin = 'C:\p2-rust-target\release'
$port = 4010

# 1) Test veritabanı
& "$pg\pgsql\bin\pg_ctl.exe" -D "$pg\data" status *> $null
if ($LASTEXITCODE -ne 0) { & "$pg\pgsql\bin\pg_ctl.exe" -D "$pg\data" -l "$pg\pg.log" -o "-p 55432" start | Out-Null; Start-Sleep 3 }

# 2) Yerel sunucu (aynı adla zaten çalışıyorsa yeniden başlatılmaz)
if (-not (Get-Process srv-deneme -ErrorAction SilentlyContinue)) {
  $env_ = Get-Content "$pg\test-env.json" -Raw | ConvertFrom-Json
  # Az önce kapatılan sunucunun dosyası bir süre kilitli kalabilir: birkaç kez dene.
  for ($i = 0; $i -lt 10; $i++) { try { Copy-Item "$bin\p2-server.exe" "$pg\srv-deneme.exe" -Force; break } catch { if ($i -eq 9) { throw }; Start-Sleep -Milliseconds 700 } }
  # Yeni derlemenin göçlerini test veritabanına uygula (sunucu başlamadan önce; uygulanmış göçler atlanır).
  $mig = New-Object System.Diagnostics.ProcessStartInfo "$pg\srv-deneme.exe", 'migrate'
  $mig.UseShellExecute = $false; $mig.CreateNoWindow = $true
  foreach ($k in 'DATABASE_URL','JWT_ACCESS_SECRET','JWT_REFRESH_SECRET') { $mig.EnvironmentVariables[$k] = $env_.$k }
  $migProc = [System.Diagnostics.Process]::Start($mig); $migProc.WaitForExit()
  if ($migProc.ExitCode -ne 0) { throw "Göç uygulanamadı (çıkış kodu $($migProc.ExitCode))" }
  $psi = New-Object System.Diagnostics.ProcessStartInfo "$pg\srv-deneme.exe"
  $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
  foreach ($k in 'DATABASE_URL','JWT_ACCESS_SECRET','JWT_REFRESH_SECRET') { $psi.EnvironmentVariables[$k] = $env_.$k }
  $psi.EnvironmentVariables['PORT'] = "$port"; $psi.EnvironmentVariables['HOST'] = '127.0.0.1'
  $psi.EnvironmentVariables['P2_SERVER_VERSION'] = '1.0.0'
  [System.Diagnostics.Process]::Start($psi) | Out-Null
  for ($i = 0; $i -lt 40; $i++) { try { if ((Invoke-WebRequest "http://127.0.0.1:$port/health" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200) { break } } catch {}; Start-Sleep -Milliseconds 500 }
}

# 3) Ayrı profil (ilk açılışta yerel sunucu adresi yazılır)
# Her oturum kendi profilinde (tek örnek kilidi profile göre): aynı anda açık kalabilirler.
$profile = if ($Oturum -eq '1') { "$pg\deneme-profil" } else { "$pg\deneme-profil-$Oturum" }
if (-not (Test-Path "$profile\settings.json")) {
  New-Item -ItemType Directory -Force $profile | Out-Null
  [IO.File]::WriteAllText("$profile\settings.json", "{`"mode`":`"client`",`"apiUrl`":`"http://127.0.0.1:$port`"}")
}

# 4) İstemci
$c = New-Object System.Diagnostics.ProcessStartInfo "$bin\p2-client.exe"
$c.UseShellExecute = $false
# Kendi (gizli) konsolunda başlar: bu PowerShell penceresi kapanınca istemci de kapanmasın.
$c.CreateNoWindow = $true
$c.EnvironmentVariables['P2_PROFILE_DIR'] = $profile
$c.EnvironmentVariables['P2_NO_ELECTRON_IMPORT'] = '1'
[System.Diagnostics.Process]::Start($c) | Out-Null
Write-Host "P2 yerel deneme $Oturum. oturum açıldı: http://127.0.0.1:$port"
