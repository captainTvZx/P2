// Faz 8 uçtan uca testi — kurulum paketlerinin yayın düzeni ve sunucunun güncelleme sunumu.
// Hiçbir kurulum ÇALIŞTIRILMAZ; kullanıcının %APPDATA%\P2, %APPDATA%\P2-Rust, ProgramData\P2 ve kurulu
// uygulamalarına dokunulmaz (her şey C:/p2-pg/faz8 altında).
//
// Önkoşul: test PostgreSQL (:55432, C:/p2-pg/test-env.json), `cargo build --release -p p2-client
// -p p2-server`, paketler: `node packaging/build-installers.mjs` (C:/p2-rust-target/dist).
//   node tools/faz8-test.mjs
//
// [1] Yayın: packaging/publish.mjs → geçici P2_UPDATE_DIR; Rust sunucusu (:4161) /updates/client|server/…
//     latest.yml + kurulum exe'sini sunar; sha512/boy tutar; kök düzen (eski istemciler/sunucular).
// [2] Güvenlik: P2_PROFILE_DIR verilip P2_ELECTRON_PROFILE_DIR boş bırakılırsa `p2-client
//     --p2-import-electron` gerçek Electron profiline bakmaz.
// (Geçişteki Electron köprüsü + içe alma adımları gerçek Electron gerektiriyordu; Electron depodan
// kaldırıldı. İçe alma kuralları p2-client-core migrate.rs birim testlerinde.)
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { REPO, startRust } from './lib.mjs'

const BIN = 'C:/p2-rust-target/release/'
const DIST = 'C:/p2-rust-target/dist'
const BASE = 'C:/p2-pg/faz8'
const PORT = 4161
const version = JSON.parse(readFileSync(join(DIST, 'client', 'build-info.json'), 'utf8')).version
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE, { recursive: true })
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
const sha512 = buf => createHash('sha512').update(buf).digest('base64')

// ---------------------------------------------------------------- [1] yayın + sunum
console.log('\n[1] Yayın düzeni ve sunucu')
const UPDATES = BASE + '/updates'
execFileSync(process.execPath, [join(REPO, 'packaging/publish.mjs'), '--skip-build', '--version', version], { env: { ...process.env, P2_UPDATE_DIR: UPDATES }, stdio: 'ignore' })
process.env.P2_UPDATE_DIR = UPDATES
const server = await startRust(PORT)
const api = server.base
try {
  for (const product of ['client', 'server']) {
    const res = await fetch(`${api}/updates/${product}/latest.yml`)
    const yml = await res.text()
    const name = yml.match(/^path: (.+)$/m)?.[1]
    check(`${product}: /updates/${product}/latest.yml`, res.ok && yml.startsWith(`version: ${version}\n`), name)
    const exe = Buffer.from(await (await fetch(`${api}/updates/${product}/${name}`)).arrayBuffer())
    const local = readFileSync(join(DIST, product, name))
    check(`${product}: kurulum indirildi, sha512 + boy latest.yml ile aynı`, exe.equals(local) && yml.includes(`sha512: ${sha512(exe)}`) && yml.includes(`size: ${exe.length}`), `${(exe.length / 1048576).toFixed(1)} MB`)
  }
  const root = await (await fetch(`${api}/updates/latest.yml`)).text()
  check('kök latest.yml istemcinin (eski istemciler)', root.includes(`path: P2-Client-Setup-${version}.exe`))
  check('sunucu exe kökte de var (0.7.10 ve eski sunucular)', (await fetch(`${api}/updates/P2-Server-Setup-${version}.exe`, { method: 'HEAD' })).ok || (await fetch(`${api}/updates/P2-Server-Setup-${version}.exe`)).ok)

  // ---------------------------------------------------------------- [2] içe alma yalıtımı
  console.log('\n[2] Ayrı profilde gerçek Electron profiline bakılmıyor')
  const isolated = execFileSync(BIN + 'p2-client.exe', ['--p2-import-electron'], { env: { ...process.env, P2_PROFILE_DIR: BASE + '/empty-profile', P2_ELECTRON_PROFILE_DIR: '' }, timeout: 30000 }).toString().trim().split(/\r?\n/).pop()
  check('--p2-import-electron: rapor yok, ayar yazılmadı', isolated === 'null' && !existsSync(BASE + '/empty-profile/settings.json'))
} finally {
  server.stop()
}

console.log(`\n${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
