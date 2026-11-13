// Oyun içi overlay'in C++ parçalarını derler: enjekte edilen DLL (p2overlay64.dll) + enjektör
// (p2inject64.exe), CMake + MSVC ile (native/overlay/CMakeLists.txt; MinHook FetchContent ile
// indirilir). Çıktılar native/overlay/dist'e toplanır; Rust istemcisi geliştirmede oradan okur
// (p2-overlay-client Paths::locate), build-installers.mjs pakete overlay\ altına koyar.
//
//   node packaging/build-overlay.mjs            → yapılandır + derle + dist'e topla
//   node packaging/build-overlay.mjs --collect  → yalnız var olan derlemeyi dist'e topla
//
// cmake PATH'te yoksa Visual Studio'nun kendi CMake'i aranır (P2_CMAKE ile verilebilir).
// Hiçbir npm paketi gerekmez.
import { spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { repoRoot } from './version.mjs'

const source = join(repoRoot, 'native', 'overlay')
const build = join(source, 'build')
export const overlayDist = join(source, 'dist')
export const overlayFiles = ['p2overlay64.dll', 'p2inject64.exe']

function findCmake() {
  if (process.env.P2_CMAKE) return process.env.P2_CMAKE
  if (spawnSync('cmake', ['--version'], { stdio: 'ignore', shell: true }).status === 0) return 'cmake'
  const suffix = 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe'
  for (const base of ['C:/Program Files/Microsoft Visual Studio', 'C:/Program Files (x86)/Microsoft Visual Studio']) {
    for (const version of ['18', '2026', '2022', '17']) {
      for (const edition of ['Professional', 'Enterprise', 'Community', 'BuildTools']) {
        const candidate = join(base, version, edition, suffix)
        if (existsSync(candidate)) return candidate
      }
    }
  }
  return null
}

function run(label, command, args) {
  console.log(`\n▶ ${label}`)
  const result = spawnSync(command, args, { cwd: repoRoot, stdio: 'inherit', shell: command === 'cmake' })
  if (result.status !== 0) throw new Error(`${label} başarısız (kod ${result.status})`)
}

export function collectOverlay() {
  mkdirSync(overlayDist, { recursive: true })
  for (const name of overlayFiles) {
    const from = join(build, 'Release', name)
    if (!existsSync(from)) throw new Error(`overlay derlemesi eksik: ${from} (node packaging/build-overlay.mjs)`)
    copyFileSync(from, join(overlayDist, name))
    console.log(`kopyalandı: native/overlay/dist/${name}`)
  }
}

export function buildOverlay() {
  const cmake = findCmake()
  if (!cmake) throw new Error('cmake bulunamadı: PATH\'e ekle, P2_CMAKE ver ya da Visual Studio C++ masaüstü iş yükünü kur.')
  run('Overlay DLL + enjektör (CMake yapılandırma)', cmake, ['-S', source, '-B', build, '-A', 'x64'])
  run('Overlay DLL + enjektör (derleme)', cmake, ['--build', build, '--config', 'Release'])
  collectOverlay()
}

if (resolve(process.argv[1] || '') === resolve(repoRoot, 'packaging', 'build-overlay.mjs')) {
  if (process.argv.includes('--collect')) collectOverlay()
  else buildOverlay()
  console.log('\n✔ Overlay ikilileri hazır: native/overlay/dist')
}
