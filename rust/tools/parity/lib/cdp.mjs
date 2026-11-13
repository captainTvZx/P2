// En küçük Chrome DevTools Protocol istemcisi (Node 22+ yerleşik WebSocket).
// Yalnız parite aracının kendi başlattığı Electron örneğine bağlanır; masaüstüne girdi göndermez —
// Input.* olayları doğrudan o renderer'a gider.

export async function targets(port) {
  const r = await fetch(`http://127.0.0.1:${port}/json/list`)
  return r.json()
}

export async function waitTarget(port, match, timeout = 60000) {
  const end = Date.now() + timeout
  let last = []
  while (Date.now() < end) {
    try {
      last = await targets(port)
      const found = last.find(t => t.type === 'page' && match(t))
      if (found) return found
    } catch {}
    await new Promise(r => setTimeout(r, 250))
  }
  throw new Error(`CDP hedefi bulunamadı (port ${port}): ${last.map(t => t.url).join(', ')}`)
}

export class Cdp {
  static async connect(wsUrl) {
    const cdp = new Cdp(new WebSocket(wsUrl))
    await new Promise((resolve, reject) => {
      cdp.ws.addEventListener('open', resolve, { once: true })
      cdp.ws.addEventListener('error', e => reject(new Error('CDP bağlantısı açılamadı: ' + (e.message || wsUrl))), { once: true })
    })
    return cdp
  }
  constructor(ws) {
    this.ws = ws
    this.seq = 0
    this.pending = new Map()
    this.listeners = new Map()
    ws.addEventListener('message', event => {
      const msg = JSON.parse(event.data)
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject, method } = this.pending.get(msg.id)
        this.pending.delete(msg.id)
        if (msg.error) reject(new Error(`${method}: ${msg.error.message}`))
        else resolve(msg.result)
      } else if (msg.method) {
        for (const fn of this.listeners.get(msg.method) || []) fn(msg.params)
      }
    })
    ws.addEventListener('close', () => {
      for (const { reject, method } of this.pending.values()) reject(new Error(`${method}: CDP bağlantısı kapandı`))
      this.pending.clear()
    })
  }
  /** Her istek 30 sn'de zaman aşımına uğrar (takılan Electron koşuyu sonsuza dek bekletmesin). */
  send(method, params = {}, timeout = 30000) {
    const id = ++this.seq
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { if (this.pending.delete(id)) reject(new Error(method + ': CDP zaman aşımı')) }, timeout)
      this.pending.set(id, { resolve: v => { clearTimeout(timer); resolve(v) }, reject: e => { clearTimeout(timer); reject(e) }, method })
      this.ws.send(JSON.stringify({ id, method, params }))
    })
  }
  on(method, fn) {
    if (!this.listeners.has(method)) this.listeners.set(method, [])
    this.listeners.get(method).push(fn)
  }
  /** Sayfada ifade değerlendirir; değer JSON olarak döner. Hata fırlatırsa istisna. */
  async eval(expression, { awaitPromise = true } = {}) {
    const r = await this.send('Runtime.evaluate', { expression, awaitPromise, returnByValue: true, userGesture: true })
    if (r.exceptionDetails) throw new Error('sayfa hatası: ' + (r.exceptionDetails.exception?.description || r.exceptionDetails.text))
    return r.result?.value
  }
  close() { try { this.ws.close() } catch {} }
}
