// Oyun içi overlay referans görüntüsü (Electron `src/OverlayApp.tsx`) — J ajanı.
// Açık bir parite oturumunun (session.mjs start --name <ad>) sayfası overlay girişine (`?overlay=1`)
// gider; `window.p2overlay` preload'u sahte köprüyle (CDP addScriptToEvaluateOnNewDocument) değiştirilir:
// oturum/proje, test sunucusuna gerçek giriş (auth push), sabit performans örneği, bildirimler.
// Yüzey verilen boyutta (varsayılan 1280×720) siyah zeminde çizilir — Rust host'unun
// `p2-overlay --render` çıktısı da aynı zeminde birleştirilir (overlay-parity.mjs karşılaştırır).
//   node rust/tools/parity/overlay-ref.mjs --name ovJ --scenario tasks --out C:/p2-pg/parity/overlay/e-tasks.png
import { writeFileSync } from 'node:fs'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { Cdp, waitTarget } from './lib/cdp.mjs'
import { PARITY_DIR, API, parseArgs, sleep } from './lib/common.mjs'
import { SCENARIOS, PERF_SAMPLE } from './overlay-scenarios.mjs'

const args = parseArgs(process.argv.slice(2))
const W = Number(args.w || 1280), H = Number(args.h || 720)
const scenario = SCENARIOS[args.scenario || 'toolbar']
if (!scenario) throw new Error('bilinmeyen senaryo: ' + args.scenario)

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return r.json().catch(() => null)
}
const login = await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })
const token = login.accessToken
const orgs = await call('GET', '/organizations', null, token)
const org = orgs[0]
const projects = await call('GET', `/organizations/${org.id}/projects`, null, token)
const project = projects[0]
const members = await call('GET', `/organizations/${org.id}/members?projectId=${project.id}`, null, token).catch(() => [])
const cats = await call('GET', `/organizations/${org.id}/projects/${project.id}/channels`, null, token).catch(() => [])
const channels = (Array.isArray(cats) ? cats : []).flatMap(c => c.channels || []).map(c => ({ id: c.id, name: c.name, type: c.type, projectId: project.id }))
const people = (Array.isArray(members) ? members : []).map(m => ({ id: m.user?.id || m.userId || m.id, displayName: m.user?.displayName || m.displayName, avatarUrl: null, username: m.user?.username, status: 'OFFLINE', role: m.role })).filter(p => p.id)
const cfg = {
  projectId: project.id, projectName: project.name, visible: scenario.visible !== false,
  auth: { user: login.user, accessToken: token, organization: org, projects, selectedProjectId: project.id },
  snapshot: { organization: org, projects, selectedProjectId: project.id, activities: {}, voiceRooms: {}, unread: {}, channelUnread: {}, people, prefs: {}, channels },
}
const stub = `(() => {
  const CFG = ${JSON.stringify(cfg)}
  const ch = () => { const s = new Set(); return { on: h => (s.add(h), () => s.delete(h)), emit: v => s.forEach(f => f(v)) } }
  const vis = ch(), toast = ch(), dismiss = ch(), perf = ch(), focus = ch(), hot = ch(), shot = ch(), ev = ch(), push = ch()
  let visible = CFG.visible
  window.p2overlay = {
    session: () => ({ pid: 1, projectId: CFG.projectId, projectName: CFG.projectName }),
    onVisible: h => { h(visible); return vis.on(h) }, setVisible: v => { visible = v; vis.emit(v) },
    drawState() {}, onFocus: focus.on, onPerf: perf.on, onHotkey: hot.on, onScreenshot: shot.on, onEvent: ev.on,
    onToast: toast.on, onToastDismiss: dismiss.on, toastAction() {}, screenshot: async () => ({ ok: true }), setZoom() {},
    openInMain() {}, onPush: push.on, onCallState: () => () => {}, callAction() {},
    send(topic) { if (topic === 'auth-request') setTimeout(() => { push.emit({ topic: 'auth', payload: CFG.auth }); push.emit({ topic: 'snapshot', payload: CFG.snapshot }) }, 0) },
  }
  window.__ov = { vis, toast, dismiss, perf, push, ev }
})()`

const state = JSON.parse(readFileSync(join(PARITY_DIR, `session-${args.name || 'default'}.json`), 'utf8'))
const target = await waitTarget(state.cdpPort, t => t.url.startsWith(state.renderer), 5000)
const cdp = await Cdp.connect(target.webSocketDebuggerUrl)
try {
  await cdp.send('Page.enable')
  await cdp.send('Emulation.setDeviceMetricsOverride', { width: W, height: H, deviceScaleFactor: 1, mobile: false })
  // Düzen (layoutStore) ve ayarlar sayfa yüklenmeden yazılır.
  const layout = scenario.layout ? { version: 1, viewport: { w: W, h: H }, focus: scenario.layout.focus ?? null, windows: scenario.layout.windows } : null
  const settingsPatch = scenario.settings || {}
  const pre = `(() => { try {
    const k = 'p2:overlay:layout:${project.id}'
    ${layout ? `localStorage.setItem(k, ${JSON.stringify(JSON.stringify(layout))})` : 'localStorage.removeItem(k)'}
    const s = JSON.parse(localStorage.getItem('p2:settings') || '{}')
    Object.assign(s, { overlayHud: false, overlayToastCorner: 'top-right', overlayHudPosition: 'top-left', overlayHudDetails: true, overlayReduceMotion: true, overlayScale: 1, overlayOpacity: 0.92 }, ${JSON.stringify(settingsPatch)})
    localStorage.setItem('p2:settings', JSON.stringify(s))
    localStorage.setItem('p2:overlay:tasks:view', ${JSON.stringify(scenario.tasksView || 'list')})
  } catch (e) {} })()`
  const { identifier } = await cdp.send('Page.addScriptToEvaluateOnNewDocument', { source: stub + ';' + pre })
  await cdp.eval(`location.href = ${JSON.stringify(state.renderer + '/?overlay=1')}`)
  await sleep(2500)
  await cdp.send('Page.removeScriptToEvaluateOnNewDocument', { identifier })
  // Siyah zemin (Rust çıktısı da siyah üstüne birleştirilir); imleç ve metin imleci gizli.
  await cdp.eval(`(() => { const st = document.createElement('style'); st.textContent = 'html.overlay-mode, html.overlay-mode body { background: #000 !important } * { caret-color: transparent !important }'; document.head.appendChild(st) })()`)
  for (let i = 0; i < 3; i++) { await cdp.eval(`window.__ov.perf.emit(${JSON.stringify(PERF_SAMPLE)})`); await sleep(950) }
  for (const step of scenario.electron || []) {
    if (step.toast) await cdp.eval(`window.__ov.toast.emit(${JSON.stringify(step.toast)})`)
    if (step.eval) await cdp.eval(step.eval)
    if (step.click) await cdp.eval(`(() => { const el = document.querySelector(${JSON.stringify(step.click)}); el && el.click() })()`)
    if (step.hover) {
      const box = await cdp.eval(`(() => { const r = document.querySelector(${JSON.stringify(step.hover)})?.getBoundingClientRect(); return r ? [r.x + r.width / 2, r.y + r.height / 2] : null })()`)
      if (box) await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: box[0], y: box[1] })
    }
    await sleep(step.sleep || 300)
  }
  if (!(scenario.electron || []).some(s => s.hover)) await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: W - 2, y: H - 2 })
  await sleep(scenario.settle || 1500)
  const { data } = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true, captureBeyondViewport: false })
  writeFileSync(args.out || join(PARITY_DIR, 'overlay', `e-${args.scenario}.png`), Buffer.from(data, 'base64'))
  if (args.dom) {
    const dom = await cdp.eval(`(() => { const out = []; for (const el of document.querySelectorAll(${JSON.stringify(args.dom)})) { const r = el.getBoundingClientRect(); const cs = getComputedStyle(el); out.push({ cls: el.className && el.className.baseVal === undefined ? el.className : '', text: (el.innerText || '').slice(0, 40), x: r.x, y: r.y, w: r.width, h: r.height, font: cs.font, color: cs.color, bg: cs.backgroundColor, border: cs.border, radius: cs.borderRadius, pad: cs.padding, shadow: cs.boxShadow.slice(0, 80) }) } return out })()`)
    console.log(JSON.stringify(dom, null, 1))
  }
} finally {
  await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
  cdp.close()
}
console.log('tamam')
