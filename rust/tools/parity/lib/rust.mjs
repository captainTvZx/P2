// Rust tarafı: p2-client'ı geçici profil + P2_AUTOPILOT_* ile başlatır, istemci alanını yakalar.
// Girdi yalnız bu sürecin kendi penceresine (p2-wincap PostMessage; başlık "" = sürecin ilk görünür penceresi) gider; ses aygıtı açılmaz.
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { PARITY_DIR, RUST_BIN, sleep } from './common.mjs'

export class RustSession {
  static async start({ theme = 'dark', language = 'tr', api, env = {}, login = true, profile, settings = true, apiOverride } = {}) {
    const s = new RustSession()
    s.profile = profile || join(PARITY_DIR, 'profiles', `rust-${process.pid}-${Date.now()}`)
    rmSync(s.profile, { recursive: true, force: true })
    mkdirSync(s.profile, { recursive: true })
    // Sunucu adresi + dil + tema önceden yazılır (kurulum türü ekranı atlanır).
    if (settings) writeFileSync(join(s.profile, 'settings.json'), JSON.stringify({ mode: 'client', apiUrl: apiOverride || api, language, theme }))
    else writeFileSync(join(s.profile, 'settings.json'), JSON.stringify({ language, theme }))
    const full = {
      ...process.env,
      P2_PROFILE_DIR: s.profile,
      P2_NO_ELECTRON_IMPORT: '1',
      P2_NO_AUDIO_DEVICE: '1',
      P2_SILENT_NOTIFY: '1',
      P2_NO_TRAY: '1',
      P2_SPLASH: '0',
      // Ekran dışı ve odak çalmadan (P2_PARITY_ONSCREEN=1 ile kapatılır).
      ...(process.env.P2_PARITY_ONSCREEN === '1' ? {} : { P2_TEST_WINDOW_POS: '-12000,-12000' }),
      P2_AUTOPILOT_SERVER: api,
      ...(login ? { P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123' } : {}),
      ...env,
    }
    for (const [k, v] of Object.entries(full)) if (v === null || v === false) delete full[k]
    s.lines = []
    s.child = spawn(join(RUST_BIN, 'p2-client.exe'), [], { env: full, stdio: ['ignore', 'pipe', 'pipe'] })
    const onData = d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) s.lines.push(l) }
    s.child.stdout.on('data', onData)
    s.child.stderr.on('data', onData)
    return s
  }

  async waitLine(pattern, timeout = 20000) {
    const re = pattern instanceof RegExp ? pattern : new RegExp(pattern)
    const end = Date.now() + timeout
    while (Date.now() < end) {
      if (this.lines.some(l => re.test(l))) return true
      if (this.child.exitCode !== null) throw new Error('Rust istemcisi kapandı:\n' + this.lines.slice(-20).join('\n'))
      await sleep(100)
    }
    throw new Error(`satır gelmedi: ${re} — son satırlar:\n` + this.lines.slice(-12).join('\n'))
  }

  /** Adımlar: {click:[x,y]} {rightClick:[x,y]} {move:[x,y]} {key:"0x1B"} {sleep:ms} {waitLine:"re"} — yalnız kendi penceremize. */
  async run(steps = []) {
    const wincap = join(RUST_BIN, 'p2-wincap.exe')
    for (const step of steps) {
      const [op] = Object.keys(step)
      const arg = step[op]
      const pid = String(this.child.pid)
      if (op === 'sleep') await sleep(arg)
      else if (op === 'waitLine') await this.waitLine(arg, step.timeout || 20000)
      else if (op === 'click') execFileSync(wincap, ['--click', '', pid, String(arg[0]), String(arg[1])])
      else if (op === 'rightClick') execFileSync(wincap, ['--rclick', '', pid, String(arg[0]), String(arg[1])])
      else if (op === 'move') execFileSync(wincap, ['--move', '', pid, String(arg[0]), String(arg[1])])
      else if (op === 'key') execFileSync(wincap, ['--key', pid, String(arg)])
      // I ajanı: arama botunu bu adımda başlat (parity.mjs `bot.onStep`).
      else if (op === 'startBot') await this.onStartBot?.()
      else throw new Error('bilinmeyen Rust adımı: ' + op)
    }
  }

  screenshot(file) {
    execFileSync(join(RUST_BIN, 'p2-wincap.exe'), ['--client', String(this.child.pid), file], { timeout: 15000 })
    return file
  }

  async stop() {
    if (this.child && this.child.exitCode === null) {
      try { this.child.kill() } catch {}
      await sleep(500)
    }
  }
}
