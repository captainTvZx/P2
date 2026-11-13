// Electron ekranındaki her görünür öğenin hesaplanmış stillerini JSON'a döker (parite ajanları değerleri
// buradan birebir kopyalar). Katalogdaki bir ekrana gider ya da açık bir oturuma (session.mjs) bağlanır.
//   node rust/tools/parity/dom-styles.mjs --screen login [--theme dark|light] [--root ".auth-card"] [--out dosya.json]
//   node rust/tools/parity/dom-styles.mjs --session default --root ".settings-layer"      (açık oturumdaki o anki ekran)
//   ek: --tokens   :root'taki bütün --p2-*/--m-*/--ease-* değişkenlerinin çözülmüş değerleri
// Çıktı varsayılanı: C:/p2-pg/parity/dom/<ekran>-<tema>.json (+ aynı adla .txt özet: ağaç girintili, tek satır/öğe)
import { mkdirSync, writeFileSync, readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { Cdp, waitTarget } from './lib/cdp.mjs'
import { ElectronSession, startVite } from './lib/electron.mjs'
import { loadCatalog, electronSteps } from './lib/catalog.mjs'
import { PARITY_DIR, API, parseArgs, sleep } from './lib/common.mjs'

const args = parseArgs(process.argv.slice(2))
const theme = args.theme || 'dark'
const root = args.root || 'body'

// Sayfada çalışan çıkarıcı. Varsayılan değerleri (ör. margin 0px) atarak çıktıyı küçük tutar.
const EXTRACT = String.raw`(rootSel) => {
  const PROPS = ['display','position','top','left','right','bottom','z-index','box-sizing','width','height','min-width','min-height','max-width','max-height',
    'flex-direction','flex-wrap','flex-grow','flex-shrink','flex-basis','justify-content','align-items','align-self','gap','row-gap','column-gap','grid-template-columns','grid-template-rows',
    'margin-top','margin-right','margin-bottom','margin-left','padding-top','padding-right','padding-bottom','padding-left',
    'border-top-width','border-right-width','border-bottom-width','border-left-width','border-top-style','border-top-color','border-right-color','border-bottom-color','border-left-color',
    'border-top-left-radius','border-top-right-radius','border-bottom-right-radius','border-bottom-left-radius','outline','box-shadow',
    'background-color','background-image','opacity','color','font-family','font-size','font-weight','font-style','line-height','letter-spacing','text-transform','text-align','text-decoration-line','white-space','text-overflow','overflow-x','overflow-y','font-variant-numeric','font-feature-settings',
    'cursor','transform','filter','backdrop-filter','transition','animation','user-select','object-fit']
  const SKIP = { 'margin-top':'0px','margin-right':'0px','margin-bottom':'0px','margin-left':'0px','padding-top':'0px','padding-right':'0px','padding-bottom':'0px','padding-left':'0px',
    'border-top-width':'0px','border-right-width':'0px','border-bottom-width':'0px','border-left-width':'0px','border-top-style':'none','border-top-left-radius':'0px','border-top-right-radius':'0px','border-bottom-right-radius':'0px','border-bottom-left-radius':'0px',
    'box-shadow':'none','background-image':'none','opacity':'1','transform':'none','filter':'none','backdrop-filter':'none','z-index':'auto','top':'auto','left':'auto','right':'auto','bottom':'auto',
    'flex-grow':'0','flex-shrink':'1','flex-basis':'auto','flex-wrap':'nowrap','gap':'normal','row-gap':'normal','column-gap':'normal','grid-template-columns':'none','grid-template-rows':'none','min-width':'auto','min-height':'auto','max-width':'none','max-height':'none',
    'letter-spacing':'normal','text-transform':'none','text-decoration-line':'none','white-space':'normal','text-overflow':'clip','overflow-x':'visible','overflow-y':'visible','font-style':'normal','font-variant-numeric':'normal','font-feature-settings':'normal',
    'cursor':'auto','transition':'all 0s ease 0s','animation':'none 0s ease 0s 1 normal none running','outline':'rgb(0, 0, 0) none 0px','align-self':'auto','object-fit':'fill','user-select':'auto','text-align':'start','background-color':'rgba(0, 0, 0, 0)' }
  const BORDER_COLORS = ['border-top-color','border-right-color','border-bottom-color','border-left-color']
  const visible = el => { const r = el.getBoundingClientRect(); if (r.width < 1 || r.height < 1) return false; const s = getComputedStyle(el); return s.visibility !== 'hidden' && s.display !== 'none' }
  const styleOf = (el, pseudo) => {
    const cs = getComputedStyle(el, pseudo), out = {}
    for (const p of PROPS) {
      const v = cs.getPropertyValue(p)
      if (v === '' || SKIP[p] === v) continue
      if (BORDER_COLORS.includes(p) && cs.getPropertyValue(p.replace('color','width')) === '0px') continue
      out[p] = v
    }
    if (pseudo) { out.content = cs.content }
    return out
  }
  const own = el => [...el.childNodes].filter(n => n.nodeType === 3).map(n => n.textContent.trim()).join(' ').trim()
  const walk = (el, depth) => {
    if (!visible(el)) return null
    const r = el.getBoundingClientRect()
    const node = { tag: el.tagName.toLowerCase(), cls: typeof el.className === 'string' ? el.className : (el.getAttribute('class') || ''), id: el.id || undefined,
      text: own(el).slice(0, 80) || undefined, label: el.getAttribute('aria-label') || el.title || undefined,
      rect: [Math.round(r.x * 10) / 10, Math.round(r.y * 10) / 10, Math.round(r.width * 10) / 10, Math.round(r.height * 10) / 10], style: styleOf(el) }
    for (const pseudo of ['::before', '::after']) { const c = getComputedStyle(el, pseudo).content; if (c && c !== 'none' && c !== 'normal') (node.pseudo ||= {})[pseudo] = styleOf(el, pseudo) }
    if (el.tagName === 'svg') { node.svg = el.outerHTML.slice(0, 1200); return node }
    const kids = [...el.children].map(c => walk(c, depth + 1)).filter(Boolean)
    if (kids.length) node.children = kids
    return node
  }
  const rootEl = document.querySelector(rootSel)
  if (!rootEl) throw new Error('kök yok: ' + rootSel)
  return walk(rootEl, 0)
}`

const TOKENS = String.raw`(() => {
  const out = {}, cs = getComputedStyle(document.documentElement)
  const names = new Set()
  for (const sheet of document.styleSheets) { try { for (const rule of sheet.cssRules) if (rule.style) for (const n of rule.style) if (n.startsWith('--')) names.add(n) } catch {} }
  for (const n of document.documentElement.style) if (n.startsWith('--')) names.add(n)
  for (const n of [...names].sort()) out[n] = cs.getPropertyValue(n).trim()
  // Kullanılan @keyframes
  const frames = {}
  for (const sheet of document.styleSheets) { try { for (const rule of sheet.cssRules) if (rule.type === 7) frames[rule.name] = rule.cssText } catch {} }
  return { theme: document.documentElement.dataset.theme, fontScale: cs.getPropertyValue('--app-font-scale'), rootFontSize: cs.fontSize, variables: out, keyframes: frames }
})()`

function summary(node, depth = 0, lines = []) {
  const s = node.style
  const bits = [s['font-size'] && `${s['font-size']}/${s['line-height']} w${s['font-weight']}`, s.color && `c=${s.color}`, s['background-color'] && `bg=${s['background-color']}`,
    (s['padding-top'] || s['padding-left']) && `p=${[s['padding-top'] || 0, s['padding-right'] || 0, s['padding-bottom'] || 0, s['padding-left'] || 0].join(' ')}`,
    s['border-top-left-radius'] && `r=${s['border-top-left-radius']}`, s['letter-spacing'] && `ls=${s['letter-spacing']}`].filter(Boolean).join(' ')
  lines.push(`${'  '.repeat(depth)}${node.tag}${node.cls ? '.' + node.cls.trim().split(/\s+/).join('.') : ''} [${node.rect.join(',')}]${node.text ? ` "${node.text}"` : ''} ${bits}`)
  for (const c of node.children || []) summary(c, depth + 1, lines)
  return lines
}

let session, vite, cdp, name
if (args.session) {
  const state = JSON.parse(readFileSync(join(PARITY_DIR, `session-${args.session}.json`), 'utf8'))
  const target = await waitTarget(state.cdpPort, t => t.url.startsWith(state.renderer), 5000)
  cdp = await Cdp.connect(target.webSocketDebuggerUrl)
  name = `session-${args.session}`
} else {
  if (!args.screen) { console.error('--screen <id> ya da --session <ad> gerekli'); process.exit(2) }
  const catalog = loadCatalog()
  const screen = catalog.screens.find(s => s.id === args.screen)
  if (!screen) throw new Error('ekran yok: ' + args.screen)
  vite = await startVite()
  session = await ElectronSession.start({ theme, api: API, renderer: vite.url })
  await session.run(electronSteps(catalog, screen))
  await sleep(Number(args.settle || screen.settle || 1200))
  cdp = session.cdp
  name = `${args.screen}-${theme}`
}
try {
  const dir = join(PARITY_DIR, 'dom')
  mkdirSync(dir, { recursive: true })
  const out = args.out || join(dir, `${name}${args.root ? '-' + root.replace(/[^\w-]+/g, '_') : ''}.json`)
  if (args.tokens) {
    const tokens = await cdp.eval(TOKENS)
    writeFileSync(out.replace(/\.json$/, '-tokens.json'), JSON.stringify(tokens, null, 2))
    console.log(out.replace(/\.json$/, '-tokens.json'))
  }
  const tree = await cdp.eval(`(${EXTRACT})(${JSON.stringify(root)})`)
  writeFileSync(out, JSON.stringify(tree, null, 1))
  writeFileSync(out.replace(/\.json$/, '.txt'), summary(tree).join('\n'))
  console.log(out)
  console.log(out.replace(/\.json$/, '.txt'))
} finally {
  if (session) { await session.stop(); vite.stop() } else cdp.close()
}
