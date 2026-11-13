// Overlay parite senaryoları (J ajanı): Electron (overlay-ref.mjs) ve Rust (`p2-overlay --render`) aynı
// düzen JSON'unu (layoutStore biçimi, `p2:overlay:layout:<proje>`) ve aynı sabit performans örneğini kullanır.
export const PERF_SAMPLE = { fps: 144, ping: 31, cpu: 23, ram: { used: 11800, total: 32768 }, gpu: null, loss: null, game: { cpu: 14, ramMb: 2300 } }

const win = (id, x, y, w, h, extra = {}) => ({ [id]: { id, x, y, w, h, open: true, minimized: false, pinned: false, pinOpacity: 0.92, z: 1, restore: null, base: { vw: 1280, vh: 720, x, y, w, h }, ...extra } })
const one = (id, x, y, w, h, extra) => ({ focus: id, windows: win(id, x, y, w, h, extra) })

export const SCENARIOS = {
  toolbar: {},
  tasks: { layout: one('tasks', 200, 80, 640, 480) },
  'tasks-pick': { layout: one('tasks', 200, 80, 640, 480), rust: ['wait:1500', 'hit:task-status:Overlay parite: belge', 'wait:600', 'hits', 'pick:Bitti', 'wait:2000'] },
  flow: { rust: ['look:motion=full', 'tool:tasks', 'wait:2500', 'hit:tasks:new', 'wait:800', 'type:Overlay akış 1', 'key:enter', 'wait:3000', 'hit:task-status:Overlay akış 1', 'wait:800', 'pick:Sürüyor', 'wait:3000', 'tool:notes', 'wait:2500', 'hit:notes:new', 'wait:2500', 'type:Overlay notu akış', 'wait:3000', 'tool:perf', 'wait:2500', 'hit:win:perf:max', 'wait:1200', 'hit:perf:hud', 'wait:1000', 'state'] },
  'tasks-board': { layout: one('tasks', 200, 80, 760, 480), tasksView: 'board' },
  notes: { layout: one('notes', 700, 80, 420, 520) },
  perf: { layout: one('perf', 80, 60, 400, 560), rust: ['wait:1500', 'scrolls'] },
  voice: { layout: one('voice', 840, 40, 356, 420) },
  team: { layout: one('team', 840, 40, 356, 520) },
  summary: { layout: one('summary', 80, 40, 420, 600) },
  capture: { layout: one('capture', 240, 80, 640, 460) },
  library: { layout: one('library', 80, 40, 380, 560) },
  browser: { layout: one('browser', 200, 60, 820, 560) },
  chat: { layout: one('chat', 200, 60, 720, 540) },
  multi: { layout: { focus: 'notes', windows: { ...win('tasks', 120, 60, 560, 400, { z: 1 }), ...win('notes', 600, 200, 420, 400, { z: 2 }) } } },
  pinned: { visible: false, layout: { focus: null, windows: win('perf', 900, 40, 340, 420, { pinned: true, pinOpacity: 0.8 }) } },
  'pin-pop': { layout: one('perf', 300, 80, 400, 520, { pinned: true }), electron: [{ click: '.ow-pin-btn' }], rust: ['wait:1200', 'hit:win:perf:pin'] },
  hud: { visible: false, settings: { overlayHud: true } },
  toasts: {
    visible: false,
    electron: [
      { toast: { id: 't1', kind: 'info', title: 'Demo başlatıldı', text: 'P2 overlay hazır', keys: ['Shift', '2'], duration: 20000 } },
      { toast: { id: 't2', kind: 'call', title: 'Rust Bot arıyor', text: 'Sesli arama', sticky: true, tag: 'call:x', actions: [{ id: 'decline', label: 'Reddet' }, { id: 'accept', label: 'Kabul et', primary: true }] } },
    ],
    rust: ['toast-json:{"id":"t1","kind":"info","title":"Demo başlatıldı","text":"P2 overlay hazır","keys":["Shift","2"],"durationMs":20000}', 'toast-json:{"id":"t2","kind":"call","title":"Rust Bot arıyor","text":"Sesli arama","sticky":true,"tag":"call:x","actions":[{"id":"decline","label":"Reddet"},{"id":"accept","label":"Kabul et","primary":true}]}'],
  },
}
