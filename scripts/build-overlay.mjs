// Overlay ikililerini tek komutla derler: DLL + enjektör (CMake/MSVC), host eklentisi
// (node-gyp, Electron başlıkları) ve dist'e toplama. Geliştirme akışı:
//   npm run build:overlay   →   npm run dev
// Electron geliştirme modunda ikilileri native/overlay/build/Release ve host/build/Release'den
// okur (paketleme gerekmez). cmake PATH'te yoksa Visual Studio'nun kendi CMake'i aranır;
// node-gyp için Python bulunamazsa C:\Python312p2 denenir.
import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'

const root = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')
const only = process.argv[2] // 'dll' | 'host' | undefined (hepsi)

function findCmake() {
  const probe = spawnSync('cmake', ['--version'], { stdio: 'ignore', shell: true })
  if (probe.status === 0) return 'cmake'
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
function findPython() {
  if (process.env.npm_config_python || process.env.PYTHON) return process.env.npm_config_python || process.env.PYTHON
  for (const candidate of ['C:/Python312p2/python.exe', 'C:/Python312/python.exe', 'C:/Python313/python.exe']) if (existsSync(candidate)) return candidate
  return ''
}
function run(label, command, args, env = {}) {
  console.log(`\n▶ ${label}`)
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', shell: command === 'cmake' || command.endsWith('.cmd'), env: { ...process.env, ...env } })
  if (result.status !== 0) { console.error(`✖ ${label} başarısız (kod ${result.status})`); process.exit(result.status || 1) }
}

if (!only || only === 'dll') {
  const cmake = findCmake()
  if (!cmake) { console.error('cmake bulunamadı: PATH\'e ekle ya da Visual Studio C++ masaüstü iş yükünü kur.'); process.exit(1) }
  run('DLL + enjektör (CMake yapılandırma)', cmake, ['-S', 'native/overlay', '-B', 'native/overlay/build', '-A', 'x64'])
  run('DLL + enjektör (derleme)', cmake, ['--build', 'native/overlay/build', '--config', 'Release'])
}
if (!only || only === 'host') {
  const python = findPython()
  const gyp = join(root, 'node_modules', '.bin', process.platform === 'win32' ? 'node-gyp.cmd' : 'node-gyp')
  run('Host eklentisi (node-gyp, Electron başlıkları)', gyp, ['rebuild', '--directory=native/overlay/host', '--target=44.0.0', '--dist-url=https://electronjs.org/headers', '--arch=x64'], python ? { npm_config_python: python, PYTHON: python } : {})
}
run('dist\'e toplama', process.execPath, [join(root, 'scripts', 'collect-overlay.mjs')])
console.log('\n✔ Overlay ikilileri hazır. Geliştirme: npm run dev (paketleme gerekmez).')
