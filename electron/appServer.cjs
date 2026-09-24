const http = require('node:http')
const fs = require('node:fs')
const path = require('node:path')

// Paketlenmiş arayüz file:// yerine 127.0.0.1 üzerinden sunulur. Sebebi:
// file:// kökeninde gömülü oynatıcılar (YouTube) ve bazı güvenli bağlam
// gerektiren tarayıcı özellikleri çalışmıyor; localhost kökeni normal bir web
// sayfası gibi davranır. Sunucu yalnızca döngü arayüzünü dinler ve rastgele
// bir porta bağlanır.
const TYPES = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8', '.json': 'application/json; charset=utf-8', '.svg': 'image/svg+xml',
  '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp',
  '.woff': 'font/woff', '.woff2': 'font/woff2', '.ttf': 'font/ttf', '.otf': 'font/otf', '.ico': 'image/x-icon',
  '.mp3': 'audio/mpeg', '.wav': 'audio/wav', '.mp4': 'video/mp4', '.webm': 'video/webm', '.map': 'application/json',
}

// Köken (origin) sabit kalmalı: yerel ayarlar, tema ve oturum tercihleri
// tarayıcı tarafından kökene göre saklanıyor. Bu yüzden port bir kez seçilip
// kaydediliyor ve sonraki açılışlarda aynısı deneniyor.
function startAppServer(rootDir, preferredPort = 0) {
  return new Promise((resolve, reject) => {
    const server = http.createServer((request, response) => {
      let pathname = '/'
      let url = new URL('http://127.0.0.1/')
      try { url = new URL(request.url, 'http://127.0.0.1'); pathname = decodeURIComponent(url.pathname) } catch {}
      // Dizin dışına çıkma denemeleri (../) çözümlemeden sonra elenir.
      const target = path.resolve(rootDir, '.' + pathname)
      const file = target.startsWith(rootDir) && fs.existsSync(target) && fs.statSync(target).isFile()
        ? target
        : path.join(rootDir, 'index.html')
      fs.readFile(file, (error, data) => {
        if (error) { response.statusCode = 404; response.end('yok'); return }
        response.setHeader('Content-Type', TYPES[path.extname(file).toLowerCase()] || 'application/octet-stream')
        response.setHeader('Cache-Control', 'no-store')
        response.end(data)
      })
    })
    let retried = false
    server.on('error', error => {
      // Kayıtlı port başkası tarafından kullanılıyorsa boş bir port seçilir.
      if (!retried && preferredPort) { retried = true; server.listen(0, '127.0.0.1'); return }
      reject(error)
    })
    server.on('listening', () => resolve({ base: `http://127.0.0.1:${server.address().port}`, port: server.address().port, server }))
    server.listen(preferredPort, '127.0.0.1')
  })
}

module.exports = { startAppServer }
