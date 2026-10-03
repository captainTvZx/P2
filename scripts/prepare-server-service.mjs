import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = process.cwd()
const source = join(root, 'node_modules', 'node-windows', 'bin', 'winsw', 'winsw.exe')
const template = join(root, 'electron', 'service', 'P2ServerService.xml')
const output = join(root, '.server-service')
const icon = join(root, 'build', 'icon-server.png')
for (const file of [source, template, icon]) {
  if (!existsSync(file)) throw new Error(`P2 Server paket girdisi eksik: ${file}`)
}
mkdirSync(output, { recursive: true })
copyFileSync(source, join(output, 'P2ServerService.exe'))
writeFileSync(join(output, 'P2ServerService.xml'), readFileSync(template))
console.log('P2Server Windows Service sarmalayıcısı hazırlandı.')

