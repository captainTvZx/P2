# Play Through (MSIX) yerel deneme kurulumu — imzasız, Geliştirici Modu gerekir.
# .msix dosyasına çift tıklamak işe yaramaz (imzasız paket); bu betik son derlenen gevşek düzeni kaydeder.
#   powershell -ExecutionPolicy Bypass -File packaging\msix\test-install.ps1
#   powershell -ExecutionPolicy Bypass -File packaging\msix\test-install.ps1 -Remove   (kaldır)
param([switch]$Remove, [string]$Out = 'C:\p2-rust-target\dist\msix')

$ErrorActionPreference = 'Stop'
$name = 'THROUGHDUSTADASTRA.PlayThrough'

$dev = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense
if ($dev -ne 1) {
  Write-Host 'Geliştirici Modu kapalı: Ayarlar > Sistem > Geliştiriciler için > Geliştirici Modu → Açık, sonra tekrar çalıştırın.' -ForegroundColor Yellow
  Start-Process 'ms-settings:developers'
  exit 1
}

if (Get-Process -Name 'P2 Client' -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$Out*" }) {
  Write-Host 'Play Through açık. Tepsi simgesinden çıkış yapıp tekrar çalıştırın.' -ForegroundColor Yellow
  exit 1
}

$old = Get-AppxPackage -Name $name
if ($old) {
  Write-Host "Eski deneme kurulumu kaldırılıyor ($($old.Version))"
  $old | Remove-AppxPackage
}
if ($Remove) { Write-Host 'Kaldırıldı.'; exit 0 }

$stage = Get-ChildItem $Out -Directory -Filter 'stage-*' | Sort-Object Name -Descending | Select-Object -First 1
if (-not $stage) { Write-Host "Paket yok: önce  node packaging/build-msix.mjs" -ForegroundColor Yellow; exit 1 }
Add-AppxPackage -Register (Join-Path $stage.FullName 'AppxManifest.xml')
$p = Get-AppxPackage -Name $name
Write-Host "Kuruldu: Play Through $($p.Version)  ($($stage.FullName))" -ForegroundColor Green
Write-Host 'Başlat menüsünden "Play Through" ile açın.'
