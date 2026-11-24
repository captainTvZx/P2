// P2 Server (Rust) Ubuntu 24.04 paketi: P2-Server-Ubuntu-24.04-x64-<v>.tar.gz (+ .sha256).
// İçerik: install.sh (sürüm gömülü), p2-server (linux-x64 ikilisi), p2-server.service, p2-serverctl,
// uninstall.sh — deploy/linux. Node çalışma zamanı gerekmez (Electron paketinin aksine).
//
//   node packaging/linux/build-linux-server.mjs                       → derle + paketle
//   node packaging/linux/build-linux-server.mjs --binary ./p2-server  → hazır ikiliyi paketle
//   seçenekler: --version X, --out DIR (varsayılan <target>/dist/linux), --target-triple T
//
// Derleme yolları (sırayla):
//   1. Linux'ta: cargo build --release -p p2-server (yerel).
//   2. Windows'ta: `cargo zigbuild` kuruluysa (cargo install cargo-zigbuild + zig) çapraz derleme.
//   3. Windows'ta: hedef (rustup target add x86_64-unknown-linux-gnu) ve P2_LINUX_LINKER verilmişse
//      cargo build --target ile.
// Hiçbiri yoksa ne yapılacağını yazar ve 2 ile çıkar; o durumda ikiliyi bir Ubuntu makinesinde
// (ya da CI'da) `cargo build --release -p p2-server` ile derleyip --binary ile ver.
//
// Betikler CRLF ile çıkış yapılmış olabilir (core.autocrlf); pakete LF olarak yazılır.

import { createHash } from 'node:crypto'
import { execFileSync, spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve, basename } from 'node:path'
import { productVersion } from '../version.mjs'

const here = dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'))
const repo = resolve(here, '..', '..')
function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`)
  return i >= 0 && process.argv[i + 1] && !process.argv[i + 1].startsWith('--') ? process.argv[i + 1] : fallback
}
const version = productVersion(arg('version'))
const triple = arg('target-triple', 'x86_64-unknown-linux-gnu')
const targetDir = resolve(process.env.CARGO_TARGET_DIR || (process.platform === 'win32' ? 'C:/p2-rust-target' : join(repo, 'target')))
const out = resolve(arg('out', join(targetDir, 'dist', 'linux')))
const name = `P2-Server-Ubuntu-24.04-x64-${version}`
const stage = join(out, name)
const artifact = `${stage}.tar.gz`

const has = (cmd, args) => spawnSync(cmd, args, { stdio: 'ignore', shell: process.platform === 'win32' }).status === 0
const env = { ...process.env, P2_PRODUCT_VERSION: version }

function build() {
  if (process.platform === 'linux') {
    execFileSync('cargo', ['build', '--release', '-p', 'p2-server', '--target-dir', targetDir], { cwd: repo, stdio: 'inherit', env })
    return join(targetDir, 'release', 'p2-server')
  }
  if (has('cargo', ['zigbuild', '--version'])) {
    execFileSync('cargo', ['zigbuild', '--release', '-p', 'p2-server', '--target', triple, '--target-dir', targetDir], { cwd: repo, stdio: 'inherit', env, shell: process.platform === 'win32' })
    return join(targetDir, triple, 'release', 'p2-server')
  }
  const installed = spawnSync('rustup', ['target', 'list', '--installed'], { encoding: 'utf8', shell: process.platform === 'win32' }).stdout || ''
  if (installed.split(/\r?\n/).includes(triple) && process.env.P2_LINUX_LINKER) {
    const key = `CARGO_TARGET_${triple.toUpperCase().replace(/-/g, '_')}_LINKER`
    execFileSync('cargo', ['build', '--release', '-p', 'p2-server', '--target', triple, '--target-dir', targetDir], { cwd: repo, stdio: 'inherit', env: { ...env, [key]: process.env.P2_LINUX_LINKER }, shell: process.platform === 'win32' })
    return join(targetDir, triple, 'release', 'p2-server')
  }
  console.error(`Bu makinede Linux için derleme aracı yok (${triple}).
Seçenekler:
  • Ubuntu'da (ya da WSL/CI'da):  CARGO_TARGET_DIR=target P2_PRODUCT_VERSION=${version} cargo build --release -p p2-server
    sonra:  node packaging/linux/build-linux-server.mjs --binary target/release/p2-server
  • Windows'ta çapraz:  cargo install cargo-zigbuild  +  zig (https://ziglang.org)  ve bu betiği yeniden çalıştır
  • ya da:  rustup target add ${triple}  +  P2_LINUX_LINKER=<x86_64-linux-gnu-gcc>`)
  process.exit(2)
}

const binary = arg('binary') ? resolve(arg('binary')) : build()
if (!existsSync(binary)) throw new Error(`p2-server ikilisi yok: ${binary}`)
const head = readFileSync(binary).subarray(0, 4)
if (!head.equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46])) && !process.argv.includes('--allow-non-elf')) {
  throw new Error(`${binary} bir Linux (ELF) ikilisi değil`)
}

rmSync(stage, { recursive: true, force: true })
rmSync(artifact, { force: true })
mkdirSync(stage, { recursive: true })
copyFileSync(binary, join(stage, 'p2-server'))
const lf = text => text.replace(/\r\n/g, '\n')
for (const file of ['p2-server.service', 'p2-serverctl', 'uninstall.sh', 'turn-setup.sh']) {
  writeFileSync(join(stage, file), lf(readFileSync(join(repo, 'deploy', 'linux', file), 'utf8')))
}
writeFileSync(join(stage, 'install.sh'), lf(readFileSync(join(repo, 'deploy', 'linux', 'install.sh'), 'utf8')).replaceAll('__P2_VERSION__', version))

// Windows: yerleşik bsdtar (Git Bash'in GNU tar'ı "C:" yolunu uzak makine sanıyor).
const tar = process.platform === 'win32' ? join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'tar.exe') : 'tar'
// GNU tar'da çalıştırılabilir bitleri paket içinde ayarla; Windows tar'ı (bsdtar) mod veremez —
// install.sh zaten `install -m 0755` ile kopyalar ve `sudo bash install.sh` ile çalıştırılır.
if (process.platform !== 'win32') execFileSync('chmod', ['0755', join(stage, 'p2-server'), join(stage, 'p2-serverctl'), join(stage, 'install.sh'), join(stage, 'uninstall.sh'), join(stage, 'turn-setup.sh')])
execFileSync(tar, ['-czf', artifact, '-C', out, basename(stage)], { stdio: 'inherit' })
const digest = createHash('sha256').update(readFileSync(artifact)).digest('hex')
writeFileSync(`${artifact}.sha256`, `${digest}  ${basename(artifact)}\n`)
console.log(`Ubuntu paketi hazır: ${artifact}\nKurulum (sunucuda): tar -xzf ${basename(artifact)} && cd ${name} && sudo bash install.sh`)
