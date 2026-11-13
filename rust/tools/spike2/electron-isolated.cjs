// P2 Electron istemcisini AYRI bir profille başlatır (Faz 0 / Spike 2 testleri için).
// Geliştirme sürümü uygulama adını "P2" yaptığı için normalde %APPDATA%\P2 kullanır;
// bu, kurulu P2 Client'ın profiliyle aynı klasördür (oturum dosyası dahil). Burada
// userData, main.cjs yüklenmeden önce başka bir klasöre çevrilir.
//   electron electron-isolated.cjs   (ortam: P2_REPO, P2_TEST_PROFILE, P2_API_URL)
const { app } = require('electron')
const path = require('node:path')

const profile = process.env.P2_TEST_PROFILE || 'C:\\p2-pg\\electron-profile'
app.setPath('userData', profile)
app.setPath('sessionData', profile)
require(path.join(process.env.P2_REPO, 'electron', 'main.cjs'))
