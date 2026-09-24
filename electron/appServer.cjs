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

// Gömülü oynatıcı sayfası: YouTube çerçevesi buradan yükleniyor, böylece
// oynatıcının gördüğü köken http://127.0.0.1 oluyor.
const EMBED_HTML = (id) => `<!doctype html><html><head><meta charset="utf-8"><style>html,body{margin:0;height:100%;background:#000}iframe{border:0;width:100%;height:100%;display:block}</style></head><body><iframe src="https://www.youtube-nocookie.com/embed/${id}?autoplay=1&rel=0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" allowfullscreen></iframe></body></html>`

function startAppServer(rootDir) {
  return new Promise((resolve, reject) => {
    const server = http.createServer((request, response) => {
      let pathname = '/'
      let url = new URL('http://127.0.0.1/')
      try { url = new URL(request.url, 'http://127.0.0.1'); pathname = decodeURIComponent(url.pathname) } catch {}
      if (pathname === '/embed') {
        const id = String(url.searchParams.get('v') || '')
        response.setHeader('Content-Type', 'text/html; charset=utf-8')
        response.end(/^[A-Za-z0-9_-]{6,20}$/.test(id) ? EMBED_HTML(id) : '<!doctype html><body style="background:#000">')
        return
      }
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
    server.on('error', reject)
    server.listen(0, '127.0.0.1', () => resolve({ base: `http://127.0.0.1:${server.address().port}`, server }))
  })
}

module.exports = { startAppServer }
