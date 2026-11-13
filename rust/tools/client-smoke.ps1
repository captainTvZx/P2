# Dilim 1 duman testi: p2-client'ı otomatik pilotla çalıştırır (masaüstüne giriş göndermez).
# Önkoşul: test PostgreSQL + P2 sunucusu ($Server, varsayılan NestJS :4110), p2test hesabı.
#   1) temiz profil: kurulum türü → bağlan → giriş → kuruluş → çalışma alanı
#   2) aynı profil, otomatik pilot kullanıcısı YOK: kayıtlı oturum (DPAPI) yenilenip doğrudan çalışma alanı
#   3) sunucu adresi kapalı bir porta: "sunucuya bağlanamadık" (3 deneme × ~2 sn)
param([string]$Server = 'http://127.0.0.1:4110', [string]$User = 'p2test', [string]$Pass = 'Parola123')
$exe = 'C:\p2-rust-target\release\p2-client.exe'
$profile = 'C:\p2-pg\rust-client-profile'
Remove-Item $profile -Recurse -Force -ErrorAction SilentlyContinue

function Run($label, $seconds, [hashtable]$envs) {
  foreach ($k in 'P2_AUTOPILOT_SERVER','P2_AUTOPILOT_USER','P2_AUTOPILOT_PASS') { Remove-Item "Env:\$k" -ErrorAction SilentlyContinue }
  $env:P2_PROFILE_DIR = $profile
  foreach ($k in $envs.Keys) { Set-Item "Env:\$k" $envs[$k] }
  $out = "$env:TEMP\p2client-$label.txt"
  $p = Start-Process $exe -PassThru -RedirectStandardOutput $out
  Start-Sleep $seconds
  $p.Refresh()
  $mem = if (-not $p.HasExited) { "{0} MB özel / {1} MB ws" -f [math]::Round($p.PrivateMemorySize64/1MB), [math]::Round($p.WorkingSet64/1MB) } else { "süreç kapandı (kod $($p.ExitCode))" }
  if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
  "== $label ($mem)"
  Get-Content $out
}

Run '1-temiz-profil' 8 @{ P2_AUTOPILOT_SERVER = $Server; P2_AUTOPILOT_USER = $User; P2_AUTOPILOT_PASS = $Pass }
"   oturum dosyası: " + (Test-Path "$profile\session.bin") + ", ayarlar: " + (Get-Content "$profile\settings.json" -Raw).Replace("`n", ' ')
Run '2-kayitli-oturum' 6 @{}
$s = Get-Content "$profile\settings.json" -Raw | ConvertFrom-Json; $s.apiUrl = 'http://127.0.0.1:9'; $s | ConvertTo-Json | Set-Content "$profile\settings.json" -Encoding utf8
Run '3-sunucu-kapali' 12 @{}
