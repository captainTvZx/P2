// Dokümanlar ekranlarının sahte (sunucuya yazılmayan) verisi. Electron'a `mock` adımıyla (liste + klasör
// istekleri), Rust'a `P2_AUTOPILOT_DOCS_DEMO=<json>` ile aynı veri verilir. Çalıştırınca catalog.json'daki
// `documents-*` ekranlarını bu veriden yeniden üretir:  node rust/tools/parity/fixtures/docs-demo.mjs
import { readFileSync, writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const owner = { id: 'demo-owner', displayName: 'Parite Test', username: 'p2test' }
const bot = { id: 'demo-bot', displayName: 'Rust Bot', username: 'rustbot' }
const at = '2026-11-10T09:30:00.000Z'
const base = { projectId: 'demo', createdAt: at, updatedAt: at }

const canvas = {
  ...base, id: 'demo-canvas', ownerId: owner.id, owner, folderId: 'demo-folder', title: 'Oyun akışı', kind: 'CANVAS', access: 'OWNER',
  content: {
    version: 2,
    nodes: [
      { id: 'n1', type: 'BOX', x: -330, y: -150, w: 280, h: 180, text: 'Oyuncu girişi', color: '#1d2f57', borderWidth: 2, borderColor: '#000000', fontSize: 14 },
      { id: 'n2', type: 'SPHERE', x: 90, y: -150, w: 180, h: 180, text: 'Lobi', color: '#315b8a', borderWidth: 2, borderColor: '#000000', fontSize: 14 },
      { id: 'n3', type: 'TRIANGLE', x: -140, y: 110, w: 210, h: 180, text: 'Maç', color: '#2f6a5b', borderWidth: 2, borderColor: '#000000', fontSize: 14 },
      { id: 'n4', type: 'TEXT', x: 150, y: 170, w: 140, h: 37, text: 'Taslak akış', color: 'transparent', borderWidth: 0, borderColor: '#000000', fontSize: 18 },
    ],
    connections: [
      { id: 'w1', from: 'n1', fromSide: 'right', to: 'n2', toSide: 'left', label: 'bağlan' },
      { id: 'w2', from: 'n2', fromSide: 'bottom', to: 'n3', toSide: 'right' },
    ],
  },
}
const page = {
  ...base, id: 'demo-page', ownerId: owner.id, owner, folderId: null, title: 'Sürüm notları', kind: 'PAGE', access: 'OWNER',
  content: {
    version: 3, pageSize: 'A4',
    pages: [{ id: 'page-1', body: '<h1>Sürüm notları</h1><p>Bu sürümde <b>kalın</b>, <i>italik</i> ve <u>altı çizili</u> metin var.</p><p>İkinci paragraf: dokümanlar artık klasörlere ayrılabiliyor.</p><blockquote>Alıntı satırı</blockquote>' }],
  },
}
const shared = {
  ...base, id: 'demo-shared', ownerId: bot.id, owner: bot, folderId: 'demo-folder', title: 'Toplantı notları', kind: 'PAGE', access: 'READ',
  content: { version: 3, pages: [{ id: 'page-1', body: '<p>Salt okunur sayfa.</p>' }] },
}
const folders = [{ id: 'demo-folder', projectId: 'demo', name: 'Tasarım', position: 0, createdAt: at, updatedAt: at }]
const permissions = { owner, members: [owner, bot, { id: 'demo-third', displayName: 'Deniz Kaya', username: 'deniz' }], permissions: [{ userId: bot.id, access: 'WRITE' }] }

export const demos = {
  page: { documents: [page, canvas, shared], folders },
  canvas: { documents: [canvas, page, shared], folders },
}

// Geliştirme kipinde React StrictMode yükleme etkisini iki kez çalıştırır: liste + klasör isteği ikişer kez gider.
const mockSteps = demo => [demo.documents, demo.folders, demo.documents, demo.folders].map(body => ({ mock: { url: '*/projects/*/documents*', body } }))

function screen(id, title, demo, extraElectron = [], extraEnv = {}, ready = 'doküman: açıldı') {
  return {
    id, group: 'tools', owner: 'G', layout: 'workspace', title,
    electron: { steps: [{ use: 'ws' }, ...mockSteps(demo), { click: 'button::text(Dokümanlar)' }, { sleep: 1400 }, ...extraElectron] },
    rust: { env: { P2_AUTOPILOT_PAGE: 'documents', P2_AUTOPILOT_DOCS_DEMO: JSON.stringify(demo), ...extraEnv }, ready },
    verified: false,
  }
}

export const screens = [
  screen('documents-page', 'Dokümanlar — sayfa', demos.page),
  screen('documents-canvas', 'Dokümanlar — tuval', demos.canvas),
  screen('documents-doc-menu', 'Dokümanlar — doküman menüsü', demos.canvas,
    [{ click: '.documents-more' }, { sleep: 500 }], { P2_AUTOPILOT_DOCS_MENU: 'more' }),
  screen('documents-create-menu', 'Dokümanlar — yeni doküman menüsü', demos.page,
    [{ click: 'button[aria-label="Yeni doküman"]' }, { sleep: 500 }], { P2_AUTOPILOT_DOCS_MENU: 'create' }),
  screen('documents-share', 'Dokümanlar — paylaşım', demos.page,
    [{ mock: { url: '*/permissions*', body: permissions } }, { mock: { url: '*/permissions*', body: permissions } }, { click: '.documents-share-button' }, { sleep: 900 }], { P2_AUTOPILOT_DOCS_MENU: 'share', P2_AUTOPILOT_DOCS_PERMS: JSON.stringify(permissions) }),
  screen('documents-canvas-menu', 'Dokümanlar — tuval sağ tık', demos.canvas,
    [{ rightClick: '.documents-canvas-scroll' }, { sleep: 500 }], { P2_AUTOPILOT_DOCS_MENU: 'canvas' }),
]

if (process.argv[1] && process.argv[1].endsWith('docs-demo.mjs')) {
  const file = join(dirname(fileURLToPath(import.meta.url)), '..', 'catalog.json')
  const catalog = JSON.parse(readFileSync(file, 'utf8'))
  const list = catalog.screens
  for (const s of screens) {
    const i = list.findIndex(x => x.id === s.id)
    if (i >= 0) list[i] = { ...s, verified: list[i].verified ?? false }
    else {
      const after = list.findIndex(x => x.id === 'documents')
      list.splice(after + 1 + screens.indexOf(s), 0, s)
    }
  }
  writeFileSync(file, JSON.stringify(catalog, null, 2) + '\n')
  console.log('catalog: ' + screens.map(s => s.id).join(', '))
}
