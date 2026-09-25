// Yeni sürümü yayınlar: kurulum dosyasını ve latest.yml'i sunucunun güncelleme
// klasörüne kopyalar. Ekipteki uygulamalar açılışta oraya bakıp kendini günceller.
//   npm run publish:update            → önce paketler, sonra yayınlar
//   npm run publish:update -- --skip-build   → var olan paketi yayınlar
import { execSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, unlinkSync } from 'node:fs'
import { join } from 'node:path'

const root = process.cwd()
const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version
const source = join(root, 'release-package')
// Sunucu süreci bu klasörü okur (electron/main.cjs → P2_UPDATE_DIR).
const target = process.env.P2_UPDATE_DIR
  || join(process.env.ProgramData || 'C:\\ProgramData', 'P2', 'updates')

if (!process.argv.includes('--skip-build')) {
  console.log(`P2 ${version} paketleniyor…`)
  execSync('npm run dist', { stdio: 'inherit' })
}

const installer = `P2-Setup-${version}.exe`
const wanted = [installer, 'latest.yml', `${installer}.blockmap`]
const missing = wanted.slice(0, 2).filter(name => !existsSync(join(source, name)))
if (missing.length) {
  console.error(`Eksik dosya: ${missing.join(', ')}\nÖnce "npm run dist" çalıştır (latest.yml için package.json içindeki build.publish gerekli).`)
  process.exit(1)
}

mkdirSync(target, { recursive: true })
// Eski kurulum dosyaları birikmesin; ama .blockmap dosyaları kalsın:
// güncelleyici kurulu sürümün blockmap'iyle yalnızca değişen parçaları indirir.
for (const name of readdirSync(target)) {
  if (wanted.includes(name)) continue
  if (/^P2-Setup-.+\.exe$/.test(name) || name === 'latest.yml') unlinkSync(join(target, name))
}
for (const name of wanted) {
  const from = join(source, name)
  if (!existsSync(from)) continue
  copyFileSync(from, join(target, name))
  console.log(`  ${name} · ${(statSync(from).size / 1048576).toFixed(1)} MB`)
}

console.log(`\nYayınlandı: ${version} → ${target}`)
console.log('Ekipteki uygulamalar açılışta (ve 30 dakikada bir) bu sürümü görüp kendini güncelleyecek.')
