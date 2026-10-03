// Yeni sürümü yayınlar: kurulum dosyasını ve latest.yml'i sunucunun güncelleme
// klasörüne kopyalar. Ekipteki uygulamalar açılışta oraya bakıp kendini günceller.
//   npm run publish:update            → önce paketler, sonra yayınlar
//   npm run publish:update -- --skip-build   → var olan paketi yayınlar
import { execSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, unlinkSync } from 'node:fs'
import { join } from 'node:path'

const root = process.cwd()
const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version
const releaseRoot = join(root, 'release-package')
// Sunucu süreci bu klasörü okur (electron/main.cjs → P2_UPDATE_DIR).
const target = process.env.P2_UPDATE_DIR
  || join(process.env.ProgramData || 'C:\\ProgramData', 'P2', 'updates')

if (!process.argv.includes('--skip-build')) {
  console.log(`P2 ${version} paketleniyor…`)
  execSync('npm run dist', { stdio: 'inherit' })
}

const products = [
  { id: 'client', installer: `P2-Client-Setup-${version}.exe`, mirrorRoot: true },
  { id: 'server', installer: `P2-Server-Setup-${version}.exe`, mirrorRoot: false, exposeAtRoot: true },
]
for (const product of products) {
  const source = join(releaseRoot, product.id)
  const names = [product.installer, `${product.installer}.blockmap`, 'latest.yml']
  const missing = [product.installer, 'latest.yml'].filter(name => !existsSync(join(source, name)))
  if (missing.length) {
    console.error(`${product.id} paketi eksik: ${missing.join(', ')}\nÖnce "npm run dist" çalıştır.`)
    process.exit(1)
  }
  const destinations = [join(target, product.id), ...(product.mirrorRoot ? [target] : [])]
  for (const destination of destinations) {
    mkdirSync(destination, { recursive: true })
    // Önce ikili dosyalar, en son latest.yml: eksik paket hiçbir zaman ilan edilmez.
    for (const name of names.filter(name => name !== 'latest.yml')) {
      const from = join(source, name)
      if (existsSync(from)) copyFileSync(from, join(destination, name))
    }
    copyFileSync(join(source, 'latest.yml'), join(destination, 'latest.yml'))
    for (const name of readdirSync(destination)) {
      if (names.includes(name)) continue
      if (new RegExp(`^P2-${product.id === 'client' ? 'Client' : 'Server'}-Setup-.+\\.exe$`, 'i').test(name)) unlinkSync(join(destination, name))
    }
  }
  // 0.7.10 ve daha eski sunucular alt kanal yolunu tanımıyor. Sunucu EXE'sini
  // kökte de erişilebilir tut; yalnız latest.yml istemciye ait kalır.
  if (product.exposeAtRoot) {
    mkdirSync(target, { recursive: true })
    for (const name of names.filter(name => name !== 'latest.yml')) {
      const from = join(source, name)
      if (existsSync(from)) copyFileSync(from, join(target, name))
    }
  }
  console.log(`  ${product.installer} · ${(statSync(join(source, product.installer)).size / 1048576).toFixed(1)} MB`)
}

console.log(`\nYayınlandı: ${version} → ${target}`)
console.log('Açık uygulamalar sürümü anlık duyuruyla; duyuru kaçırılırsa en geç 5 dakika içinde görecek.')
