// Uyumluluk testi yardımcısı: server/ops/backup.cjs'i (Electron/Node sürümü) doğrudan çağırır.
//   node node-backup.cjs create <databaseUrl> <dataDir> <hedef.p2backup> [includeReleases]
//   node node-backup.cjs restore <databaseUrl> <dataDir> <kaynak.p2backup>
// Sonuç stdout'a tek satır JSON. Node'un taşınabilir dökümü timestamp sütunlarını yerel saat sanar;
// testler bu yüzden TZ=UTC ile çalıştırır (bkz. rust/crates/p2-server-ops/tests/db.rs).
const path = require('node:path')
const backup = require(path.join(__dirname, '..', '..', '..', 'server', 'ops', 'backup.cjs'))

async function main() {
  const [command, databaseUrl, dataDir, file, releases] = process.argv.slice(2)
  if (command === 'create') {
    const result = await backup.createBackup({ databaseUrl, dataDir, destination: file, version: 'node-test', includeReleases: releases === '1' })
    console.log(JSON.stringify(result))
  } else if (command === 'restore') {
    const result = await backup.restoreBackup({ databaseUrl, dataDir, source: file })
    console.log(JSON.stringify(result))
  } else throw new Error('Bilinmeyen komut.')
}
main().catch(error => { console.error(error?.stack || error); process.exit(1) })
