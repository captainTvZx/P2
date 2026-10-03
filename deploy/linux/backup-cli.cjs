const path = require('node:path')
const { execFile } = require('node:child_process')
const backup = require('../server/ops/backup.cjs')

const dataDir = '/var/lib/p2-server'
const command = process.argv[2]
const requested = process.argv[3]
const runSystemctl = action => new Promise((resolve, reject) => execFile('systemctl', [action, 'p2-server.service'], error => error ? reject(error) : resolve({ ok: true })))

async function main() {
  if (command === 'backup') {
    const name = `P2-Server-Backup-${new Date().toISOString().replace(/[:.]/g, '-').replace('T', '_').replace('Z', '')}.p2backup`
    const destination = requested ? path.resolve(requested) : path.join(dataDir, 'backups', name)
    const result = await backup.createBackup({ databaseUrl: process.env.DATABASE_URL, dataDir, destination, version: process.env.P2_SERVER_VERSION })
    console.log(result.path)
    return
  }
  if (command === 'restore') {
    if (!requested) throw new Error('Yedek dosyası belirtilmedi.')
    await backup.restoreBackup({ databaseUrl: process.env.DATABASE_URL, dataDir, source: requested, stop: () => runSystemctl('stop'), start: () => runSystemctl('start') })
    console.log('Yedek başarıyla yüklendi.')
    return
  }
  throw new Error('Bilinmeyen komut.')
}
main().catch(error => { console.error(error?.stack || error); process.exit(1) })
