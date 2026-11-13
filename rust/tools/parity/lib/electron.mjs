// Electron tarafı: Vite (renderer) + yalıtılmış Electron örneği + CDP ile adım yürütücü.
import { spawn } from 'node:child_process'
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { createServer } from 'node:net'
import { Cdp, waitTarget } from './cdp.mjs'
import { REPO, PARITY_DIR, sleep, log } from './common.mjs'

export async function freePort() {
  return new Promise((resolve, reject) => {
    const s = createServer()
    s.listen(0, '127.0.0.1', () => { const { port } = s.address(); s.close(() => resolve(port)) })
    s.on('error', reject)
  })
}

async function waitHttp(url, timeout = 60000) {
  const end = Date.now() + timeout
  while (Date.now() < end) {
    try { const r = await fetch(url); if (r.ok) return } catch {}
    await sleep(250)
  }
  throw new Error('yanıt yok: ' + url)
}

/** Vite geliştirme sunucusu (renderer). Var olan bir adres verilirse (P2_PARITY_RENDERER) yeniden kullanılır. */
export async function startVite() {
  if (process.env.P2_PARITY_RENDERER) return { url: process.env.P2_PARITY_RENDERER, stop() {} }
  const port = await freePort()
  const child = spawn(process.execPath, [join(REPO, 'node_modules/vite/bin/vite.js'), '--port', String(port), '--strictPort', '--host', '127.0.0.1', '--clearScreen', 'false'], { cwd: REPO, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true })
  let output = ''
  child.stdout.on('data', d => { output += d })
  child.stderr.on('data', d => { output += d })
  const url = `http://127.0.0.1:${port}`
  try { await waitHttp(url + '/', 60000) } catch (e) { child.kill(); throw new Error('Vite açılmadı:\n' + output) }
  log(`vite: ${url}`)
  return { url, child, stop() { try { child.kill() } catch {} } }
}

// Sayfaya yerleştirilen yardımcılar: seçici sözdizimi ve görünürlük.
//   "css"                    ilk görünür eşleşme
//   "css::text(Metin)"       innerText'i Metin'i içeren en küçük görünür eşleşme
//   "text=Metin"             metni tam olarak Metin olan en içteki görünür öğe
//   "...::nth(2)"            (sonda) bulunan görünür eşleşmelerin 3.'sü (0 tabanlı)
const PAGE_HELPERS = String.raw`
window.__parity = (() => {
  const visible = el => { const r = el.getBoundingClientRect(); if (r.width < 1 || r.height < 1) return false; const s = getComputedStyle(el); return s.visibility !== 'hidden' && s.display !== 'none' && Number(s.opacity) > 0.01 }
  function all(spec) {
    let nth = 0
    const m = /::nth\((\d+)\)$/.exec(spec); if (m) { nth = Number(m[1]); spec = spec.slice(0, m.index) }
    let list
    if (spec.startsWith('text=')) {
      const want = spec.slice(5).trim()
      list = [...document.querySelectorAll('body *')].filter(el => visible(el) && (el.innerText || '').trim() === want)
      list = list.filter(el => ![...el.children].some(c => list.includes(c)))
    } else {
      const t = /^(.*)::text\((.*)\)$/.exec(spec)
      const css = t ? (t[1] || '*') : spec
      list = [...document.querySelectorAll(css)].filter(visible)
      if (t) { list = list.filter(el => [el.innerText, el.getAttribute('aria-label'), el.title].some(v => (v || '').includes(t[2]))); list.sort((a, b) => { const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect(); return ra.width * ra.height - rb.width * rb.height }) }
    }
    return { list, nth }
  }
  function q(spec) { const { list, nth } = all(spec); return list[nth] || null }
  function rect(spec) { const el = q(spec); if (!el) return null; el.scrollIntoView({ block: 'nearest', inline: 'nearest' }); const r = el.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2, w: r.width, h: r.height } }
  return { q, all: s => all(s).list, rect, visible }
})();
`

const KEYS = {
  Enter: [13, 'Enter', '\r'], Escape: [27, 'Escape'], Tab: [9, 'Tab'], Backspace: [8, 'Backspace'], Delete: [46, 'Delete'],
  ArrowDown: [40, 'ArrowDown'], ArrowUp: [38, 'ArrowUp'], ArrowLeft: [37, 'ArrowLeft'], ArrowRight: [39, 'ArrowRight'],
  Space: [32, 'Space', ' '], ',': [188, 'Comma', ','], '/': [191, 'Slash', '/'], Home: [36, 'Home'], End: [35, 'End'], F2: [113, 'F2'],
}

/** Electron oturumu: başlatır, CDP bağlar, adımları yürütür, görüntü alır. */
// Test pencerelerinin ekran dışı konumu (Electron ve Rust aynı).
export const OFFSCREEN = [-12000, -12000]

export class ElectronSession {
  static async start({ theme = 'dark', language = 'tr', api, renderer, profile, keepProfile = false, x, y, fakeMedia = process.env.P2_PARITY_REAL_MEDIA !== "1" } = {}) {
    const s = new ElectronSession()
    s.api = api
    s.theme = theme
    s.language = language
    s.cdpPort = await freePort()
    s.profile = profile || join(PARITY_DIR, 'profiles', `electron-${process.pid}-${Date.now()}`)
    if (!keepProfile) rmSync(s.profile, { recursive: true, force: true })
    mkdirSync(s.profile, { recursive: true })
    // Geçici uygulama klasörü: package.json (ad + sürüm kökteki gibi) + main → electron-main.cjs
    const appDir = join(s.profile, '..', `app-${process.pid}-${s.cdpPort}`)
    mkdirSync(appDir, { recursive: true })
    const rootPkg = JSON.parse(readFileSync(join(REPO, 'package.json'), 'utf8'))
    writeFileSync(join(appDir, 'package.json'), JSON.stringify({ name: rootPkg.name, version: rootPkg.version, main: 'main.cjs', private: true }))
    writeFileSync(join(appDir, 'main.cjs'), `require(${JSON.stringify(join(PARITY_DIR_SRC(), 'electron-main.cjs'))})\n`)
    s.appDir = appDir
    const env = { ...process.env, P2_PARITY_PROFILE: s.profile, P2_PARITY_CDP_PORT: String(s.cdpPort), P2_PARITY_RENDERER: renderer, P2_PARITY_API: api }
    // Ekran dışı (P2_PARITY_ONSCREEN=1 değilse): kullanıcının masaüstünü meşgul etmez.
    if (!Number.isFinite(x) && process.env.P2_PARITY_ONSCREEN !== '1') { x = OFFSCREEN[0]; y = OFFSCREEN[1] }
    if (Number.isFinite(x)) { env.P2_PARITY_WIN_X = String(x); env.P2_PARITY_WIN_Y = String(y) }
    // Varsayılan: bütün oturumlar aygıtsız sahte medyayla (Windows ses servisi bir kez çöktü; gerçek ses aygıtı hiç açılmaz).
    if (fakeMedia) env.P2_PARITY_FAKE_MEDIA = '1'
    delete env.ELECTRON_RUN_AS_NODE
    const exe = join(REPO, 'node_modules/electron/dist/electron.exe')
    s.child = spawn(exe, [appDir], { cwd: REPO, env, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: false })
    s.lines = []
    const onData = d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) s.lines.push(l) }
    s.child.stdout.on('data', onData)
    s.child.stderr.on('data', onData)
    const target = await waitTarget(s.cdpPort, t => t.url.startsWith(renderer), 60000).catch(e => { throw new Error(e.message + '\n' + s.lines.slice(-30).join('\n')) })
    s.cdp = await Cdp.connect(target.webSocketDebuggerUrl)
    await s.cdp.send('Page.enable')
    await s.cdp.send('Runtime.enable')
    // Pencere odakta değilken de :focus / imleç / odak halkası çizilsin (Rust penceresi odaklı açılır).
    await s.cdp.send('Emulation.setFocusEmulationEnabled', { enabled: true }).catch(() => {})
    const appearance = { themeId: theme === 'light' ? 'kagit' : theme === 'dark' ? 'p2-mavi' : theme, custom: null, font: 'inter', fontScale: 85, v: 2 }
    const boot = `(() => { try {
      localStorage.setItem('p2:mode', 'client'); localStorage.setItem('p2:apiUrl', sessionStorage.getItem('p2:parity-api') || ${JSON.stringify(api)});
      if (sessionStorage.getItem('p2:parity-no-mode')) localStorage.removeItem('p2:mode');
      const cur = JSON.parse(localStorage.getItem('p2:settings') || '{}');
      cur.language = ${JSON.stringify(language)}; cur.appearance = ${JSON.stringify(appearance)};
      localStorage.setItem('p2:settings', JSON.stringify(cur));
    } catch (e) {} })();` + PAGE_HELPERS
    await s.cdp.send('Page.addScriptToEvaluateOnNewDocument', { source: boot })
    await s.reload()
    return s
  }

  async reload() {
    await this.cdp.send('Page.reload', { ignoreCache: false })
    await sleep(300)
    const end = Date.now() + 30000
    while (Date.now() < end) {
      try { if (await this.cdp.eval(`document.readyState === 'complete' && !!window.__parity && !!document.querySelector('#root > *')`)) break } catch {}
      await sleep(150)
    }
    await sleep(400)
  }

  async rect(spec) { return this.cdp.eval(`window.__parity.rect(${JSON.stringify(spec)})`) }
  async exists(spec) { return this.cdp.eval(`!!window.__parity.q(${JSON.stringify(spec)})`) }

  async waitFor(spec, timeout = 15000) {
    const end = Date.now() + timeout
    while (Date.now() < end) {
      try { if (await this.exists(spec)) return true } catch {}
      await sleep(120)
    }
    throw new Error(`bulunamadı: ${spec}`)
  }

  async mouse(type, x, y, button = 'none', clickCount = 0) {
    await this.cdp.send('Input.dispatchMouseEvent', { type, x, y, button, clickCount, buttons: type === 'mousePressed' ? (button === 'right' ? 2 : 1) : 0 })
  }
  async click(spec, { button = 'left', count = 1 } = {}) {
    const r = typeof spec === 'string' ? await this.rect(spec) : { x: spec[0], y: spec[1] }
    if (!r) throw new Error(`tıklanacak öğe yok: ${spec}`)
    await this.mouse('mouseMoved', r.x, r.y)
    for (let i = 1; i <= count; i++) {
      await this.mouse('mousePressed', r.x, r.y, button, i)
      await this.mouse('mouseReleased', r.x, r.y, button, i)
    }
    return r
  }
  async hover(spec) {
    const r = typeof spec === 'string' ? await this.rect(spec) : { x: spec[0], y: spec[1] }
    if (!r) throw new Error(`öğe yok: ${spec}`)
    await this.mouse('mouseMoved', r.x, r.y)
  }
  async key(combo) {
    const parts = combo.split('+')
    const name = parts.pop()
    const mods = { alt: 1, ctrl: 2, meta: 4, shift: 8 }
    const modifiers = parts.reduce((m, p) => m | (mods[p.toLowerCase()] || 0), 0)
    const [vk, code, text] = KEYS[name] || [name.toUpperCase().charCodeAt(0), name.length === 1 ? 'Key' + name.toUpperCase() : name, name.length === 1 && !modifiers ? name : undefined]
    await this.cdp.send('Input.dispatchKeyEvent', { type: text ? 'keyDown' : 'rawKeyDown', key: name.length === 1 ? name : (KEYS[name]?.[1] || name), code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers, text })
    await this.cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: name.length === 1 ? name : (KEYS[name]?.[1] || name), code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers })
  }
  // Sonraki eşleşen isteği sahte yanıtla karşılar (ör. 2FA adımı): {url: kalıp, status, body}. Tek seferlik.
  async mock({ url, status = 200, body = {} }) {
    await this.cdp.send('Fetch.enable', { patterns: [{ urlPattern: url, requestStage: 'Request' }] })
    if (!this.mockHooked) {
      this.mockHooked = true
      this.cdp.on('Fetch.requestPaused', async ev => {
        // CORS ön isteği (OPTIONS) sahte yanıtı tüketmesin
        const m = ev.request.method === 'OPTIONS' ? null : this.mocks.shift()
        if (!m) { await this.cdp.send('Fetch.continueRequest', { requestId: ev.requestId }).catch(() => {}); return }
        const headers = [{ name: 'content-type', value: 'application/json' }, { name: 'access-control-allow-origin', value: '*' }]
        await this.cdp.send('Fetch.fulfillRequest', { requestId: ev.requestId, responseCode: m.status, responseHeaders: headers, body: Buffer.from(JSON.stringify(m.body)).toString('base64') }).catch(() => {})
        if (!this.mocks.length) await this.cdp.send('Fetch.disable').catch(() => {})
      })
    }
    ;(this.mocks ||= []).push({ status, body })
  }
  async insert(text) { await this.cdp.send('Input.insertText', { text }) }

  /** Oturum açık değilse giriş yapar ve çalışma alanını bekler. */
  async login(user = 'p2test', pass = 'Parola123') {
    const end = Date.now() + 30000
    let state = 'wait'
    while (Date.now() < end) {
      state = await this.cdp.eval(`document.querySelector('.auth-card form') ? 'auth' : (document.querySelector('.project-rail') ? 'ws' : 'wait')`).catch(() => 'wait')
      if (state !== 'wait') break
      await sleep(200)
    }
    if (state === 'ws') return
    if (state !== 'auth') throw new Error('ne giriş ekranı ne çalışma alanı göründü')
    const inputs = await this.cdp.eval(`[...document.querySelectorAll('.auth-card input:not([type=checkbox])')].length`)
    await this.click('.auth-card input:not([type=checkbox])::nth(0)')
    await this.insert(user)
    await this.click('.auth-card input[type=password]')
    await this.insert(pass)
    if (inputs < 2) throw new Error('giriş formu beklenmedik')
    await this.click('.auth-card form > button[type=submit], .auth-card form > button')
    const until = Date.now() + 20000
    while (Date.now() < until) {
      if (await this.exists('.project-rail')) { await sleep(1200); return }
      await sleep(200)
    }
    throw new Error('Electron girişi tamamlanmadı')
  }

  /** Katalog adımlarını yürütür. */
  async run(steps = []) {
    for (const step of steps) {
      const [op] = Object.keys(step).filter(k => k !== 'timeout' && k !== 'optional')
      const arg = step[op]
      try {
        switch (op) {
          case 'login': await this.login(arg?.user, arg?.pass); break
          case 'logout': await this.cdp.eval(`(async () => { await window.p2?.session?.clear?.(); localStorage.removeItem('p2:lastOrg'); })()`); await this.reload(); break
          case 'reload': await this.reload(); break
          case 'wait': await this.waitFor(arg, step.timeout || 15000); break
          case 'waitGone': { const end = Date.now() + (step.timeout || 15000); while (Date.now() < end && await this.exists(arg)) await sleep(120); break }
          case 'click': await this.click(arg); break
          case 'dblclick': await this.click(arg, { count: 2 }); break
          case 'rightClick': await this.click(arg, { button: 'right' }); break
          case 'clickAt': await this.click(arg); break
          case 'hover': await this.hover(arg); break
          case 'type': await this.click(arg[0]); await this.insert(arg[1]); break
          case 'insert': await this.insert(arg); break
          case 'key': await this.key(arg); break
          case 'sleep': await sleep(arg); break
          case 'eval': await this.cdp.eval(arg); break
          case 'mock': await this.mock(arg); break
          case 'session': await this.cdp.eval(`(() => { const v = ${JSON.stringify(arg)}; for (const [k, x] of Object.entries(v)) x === null ? sessionStorage.removeItem(k) : sessionStorage.setItem(k, x) })()`); break
          case 'mouseAway': await this.mouse('mouseMoved', 1279, 799); break
          // I ajanı: arama botunu bu adımda başlat (gelen arama; parity.mjs `bot.onStep`).
          case 'startBot': await this.onStartBot?.(); break
          default: throw new Error('bilinmeyen adım: ' + op)
        }
      } catch (e) {
        if (step.optional) continue
        throw new Error(`adım ${JSON.stringify(step)}: ${e.message}`)
      }
    }
  }

  async screenshot(file) {
    const { data } = await this.cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true, captureBeyondViewport: false })
    writeFileSync(file, Buffer.from(data, 'base64'))
    return file
  }

  async stop() {
    try { this.cdp?.close() } catch {}
    if (this.child && this.child.exitCode === null) {
      // app.quit() değil: before-quit'teki sunucu durdurma kodu hiç çalışmasın; yalnız kendi süreç ağacımız.
      try { spawn('taskkill', ['/PID', String(this.child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' }) } catch {}
      await sleep(700)
    }
    try { rmSync(this.appDir, { recursive: true, force: true }) } catch {}
  }
}

function PARITY_DIR_SRC() { return join(REPO, 'rust', 'tools', 'parity') }
