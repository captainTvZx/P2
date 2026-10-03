import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
const version = pkg.version
const nodeVersion = '22.23.3'
const nodeFile = `node-v${nodeVersion}-linux-x64.tar.xz`
const nodeBase = `https://nodejs.org/dist/v${nodeVersion}`
const out = join(root, 'release-package', 'linux')
const stage = join(out, `P2-Server-Ubuntu-24.04-x64-${version}`)
const appDir = join(stage, 'app')
const artifact = `${stage}.tar.gz`

function command(file, args, options = {}) {
  execFileSync(file, args, { cwd: root, stdio: 'inherit', windowsHide: true, shell: process.platform === 'win32' && file.toLowerCase().endsWith('.cmd'), ...options })
}
async function download(url, destination) {
  const response = await fetch(url)
  if (!response.ok) throw new Error(`${url} indirilemedi: HTTP ${response.status}`)
  writeFileSync(destination, Buffer.from(await response.arrayBuffer()))
}

rmSync(stage, { recursive: true, force: true })
rmSync(artifact, { force: true })
mkdirSync(appDir, { recursive: true })

command(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['run', 'build:server'])
command(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['run', 'db:generate'])

const runtimeDependencies = ['@nestjs/common', '@nestjs/core', '@nestjs/jwt', '@nestjs/platform-express', '@nestjs/platform-socket.io', '@nestjs/websockets', '@prisma/client', 'argon2', 'class-transformer', 'class-validator', 'dotenv', 'pg', 'reflect-metadata', 'rxjs', 'socket.io']
const linuxPackage = {
  name: 'p2-server-linux', version, private: true, type: 'commonjs',
  dependencies: Object.fromEntries(runtimeDependencies.map(name => [name, pkg.dependencies[name]])),
}
writeFileSync(join(appDir, 'package.json'), JSON.stringify(linuxPackage, null, 2))
command(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['install', '--omit=dev', '--ignore-scripts', '--no-audit', '--no-fund', '--package-lock=false', '--os=linux', '--cpu=x64'], { cwd: appDir })

mkdirSync(join(appDir, 'server'), { recursive: true })
cpSync(join(root, 'server', 'dist'), join(appDir, 'server', 'dist'), { recursive: true })
cpSync(join(root, 'server', 'prisma'), join(appDir, 'server', 'prisma'), { recursive: true })
cpSync(join(root, 'server', 'ops'), join(appDir, 'server', 'ops'), { recursive: true })
cpSync(join(root, 'node_modules', '@prisma', 'client'), join(appDir, 'node_modules', '@prisma', 'client'), { recursive: true, force: true })
cpSync(join(root, 'node_modules', '.prisma'), join(appDir, 'node_modules', '.prisma'), { recursive: true, force: true })

mkdirSync(join(appDir, 'tools'), { recursive: true })
for (const file of ['migrate.cjs', 'backup-cli.cjs']) cpSync(join(root, 'deploy', 'linux', file), join(appDir, 'tools', file))
for (const file of ['p2-server.service', 'p2-serverctl', 'uninstall.sh']) cpSync(join(root, 'deploy', 'linux', file), join(stage, file))
const installer = readFileSync(join(root, 'deploy', 'linux', 'install.sh'), 'utf8').replaceAll('__P2_VERSION__', version)
writeFileSync(join(stage, 'install.sh'), installer)

const nodeArchive = join(out, nodeFile)
const checksums = join(out, `SHASUMS256-v${nodeVersion}.txt`)
mkdirSync(out, { recursive: true })
if (!existsSync(nodeArchive)) await download(`${nodeBase}/${nodeFile}`, nodeArchive)
await download(`${nodeBase}/SHASUMS256.txt`, checksums)
const expectedLine = readFileSync(checksums, 'utf8').split(/\r?\n/).find(line => line.endsWith(`  ${nodeFile}`))
if (!expectedLine) throw new Error(`${nodeFile} için resmi SHA-256 bulunamadı.`)
const actual = createHash('sha256').update(readFileSync(nodeArchive)).digest('hex')
if (actual !== expectedLine.split(/\s+/)[0]) throw new Error('Node.js Linux çalışma zamanı SHA-256 doğrulaması başarısız.')
cpSync(nodeArchive, join(stage, 'node-runtime.tar.xz'))

const tar = process.platform === 'win32' ? 'tar.exe' : 'tar'
command(tar, ['-czf', artifact, '-C', out, stage.split(/[\\/]/).pop()])
const digest = createHash('sha256').update(readFileSync(artifact)).digest('hex')
writeFileSync(`${artifact}.sha256`, `${digest}  ${artifact.split(/[\\/]/).pop()}\n`)
console.log(`Ubuntu paketi hazır: ${artifact}`)
