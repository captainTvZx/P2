import { spawn } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const root = process.cwd()
const profile = mkdtempSync(join(tmpdir(), 'p2-coexist-'))
const env = { ...process.env }
delete env.ELECTRON_RUN_AS_NODE
const children = []
const launch = (exe, port, name, extraArgs = []) => {
  const child = spawn(exe, [...extraArgs, `--remote-debugging-port=${port}`, `--user-data-dir=${join(profile, name)}`], { stdio: 'ignore', env })
  children.push(child)
  return child
}
const cleanup = () => { for (const child of children) { try { child.kill() } catch {} } try { rmSync(profile, { recursive: true, force: true }) } catch {} }
process.on('exit', cleanup)
launch(join(root, 'release-package', 'server', 'win-unpacked', 'P2 Server.exe'), 9361, 'server', ['--p2-test-multiple'])
await new Promise(resolve => setTimeout(resolve, 1200))
launch(join(root, 'release-package', 'client', 'win-unpacked', 'P2 Client.exe'), 9362, 'client')

async function page(port, match) {
  for (let attempt = 0; attempt < 40; attempt++) {
    await new Promise(resolve => setTimeout(resolve, 500))
    try {
      const items = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
      const found = items.find(item => item.type === 'page' && item.url.includes(match))
      if (found) return found
    } catch {}
  }
  return null
}
const [server, client] = await Promise.all([page(9361, 'server-console.html'), page(9362, 'index.html')])
console.log(`${server ? 'OK  ' : 'FAIL'} P2 Server Console açık`)
console.log(`${client ? 'OK  ' : 'FAIL'} P2 Client aynı anda açık`)
cleanup()
process.exitCode = server && client ? 0 : 1
