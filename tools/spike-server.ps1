# Rust sunucusunu (cargo run -p p2-server) yerel test veritabanıyla başlatır.
#   powershell -ExecutionPolicy Bypass -File tools\spike-server.ps1            (sunucuyu aç; Ctrl+C ile kapanır)
#   powershell -ExecutionPolicy Bypass -File tools\spike-server.ps1 -StopDb    (test veritabanını kapat)
# Test veritabanı C:\p2-pg altındadır (taşınabilir PostgreSQL 17, servis değil). Ekip sunucusuna dokunmaz.
param([int]$Port = 4120, [switch]$StopDb)
$ErrorActionPreference = 'Stop'
$pg = 'C:\p2-pg'; $bin = "$pg\pgsql\bin"; $data = "$pg\data"
if (-not (Test-Path "$bin\pg_ctl.exe")) { throw "Test veritabanı bulunamadı: $pg (Faz 0 kurulumu)." }

if ($StopDb) { & "$bin\pg_ctl.exe" -D $data stop -m fast; return }

# 1) Veritabanı kapalıysa arka planda aç (pg_ctl'i ayrı süreçte başlat; aksi halde kabuk takılı kalıyor).
& "$bin\pg_isready.exe" -h 127.0.0.1 -p 55432 *> $null
if ($LASTEXITCODE -ne 0) {
  Write-Host 'Test veritabanı açılıyor (127.0.0.1:55432)...'
  Start-Process "$bin\pg_ctl.exe" -ArgumentList @('-D', $data, '-l', "$pg\pg.log", 'start') -WindowStyle Hidden
  for ($i = 0; $i -lt 40; $i++) { Start-Sleep -Milliseconds 250; & "$bin\pg_isready.exe" -h 127.0.0.1 -p 55432 *> $null; if ($LASTEXITCODE -eq 0) { break } }
  if ($LASTEXITCODE -ne 0) { throw "Veritabanı açılamadı; günlük: $pg\pg.log" }
}

# 2) Test JWT anahtarları: bir kez üretilir, depo dışında saklanır (yeniden başlatınca oturumlar geçerli kalsın).
$secrets = "$pg\spike-secrets.json"
if (-not (Test-Path $secrets)) {
  $rand = { -join ((1..48) | ForEach-Object { '{0:x}' -f (Get-Random -Maximum 16) }) }
  @{ access = (& $rand); refresh = (& $rand) } | ConvertTo-Json | Set-Content $secrets -Encoding utf8
}
$s = Get-Content $secrets -Raw | ConvertFrom-Json

# 3) Sunucu
$env:DATABASE_URL = 'postgresql://p2@127.0.0.1:55432/p2_spike'
$env:JWT_ACCESS_SECRET = $s.access
$env:JWT_REFRESH_SECRET = $s.refresh
$env:PORT = "$Port"; $env:HOST = '127.0.0.1'
if (-not $env:RUST_LOG) { $env:RUST_LOG = 'info' }
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) { $cargo = 'cargo' }
Write-Host "Rust sunucusu http://127.0.0.1:$Port adresinde açılıyor (Ctrl+C ile kapat)..."
Push-Location (Split-Path $PSScriptRoot -Parent)
try { & $cargo run -p p2-server --release } finally { Pop-Location }
