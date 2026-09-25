import { Controller, Get, Param, Req, Res } from '@nestjs/common'
import { createReadStream } from 'node:fs'
import { stat } from 'node:fs/promises'
import { join } from 'node:path'

// Otomatik güncelleme dosyaları: sunucuyu barındıran kişi yeni kurulum
// dosyasını buraya koyar (npm run publish:update), ekipteki uygulamalar
// açılışta buraya bakıp kendini günceller. Uç bilerek public: istemci daha
// giriş yapmadan da güncelleyebilmeli.
const root = process.env.P2_UPDATE_DIR || join(process.cwd(), '.p2-data', 'updates')
// Dizin dışına çıkma denemeleri: yalnızca düz dosya adları kabul edilir.
const safeFile = (value: string) => /^[A-Za-z0-9._-]+$/.test(value) && !value.startsWith('.') ? value : ''

@Controller('updates')
export class UpdatesController {
  @Get('latest.yml')
  async latest(@Res() response: any) {
    const file = join(root, 'latest.yml')
    try {
      const info = await stat(file)
      if (!info.isFile()) throw new Error('yok')
      response.setHeader('Content-Type', 'text/yaml; charset=utf-8')
      response.setHeader('Content-Length', info.size)
      response.setHeader('Cache-Control', 'no-store')
      createReadStream(file).pipe(response)
    } catch {
      // Henüz sürüm yayınlanmamış: istemci bunu sessizce geçer.
      response.status(404).json({ code: 'NO_UPDATE', message: 'Yayınlanmış sürüm yok.' })
    }
  }

  // Kurulum dosyası 150 MB'ın üzerinde; belleğe alınmaz, akıtılır. Kısmi
  // indirme (Range) desteklenir ki güncelleyici kesilen indirmeyi sürdürebilsin.
  @Get(':file')
  async download(@Param('file') name: string, @Req() request: any, @Res() response: any) {
    const safe = safeFile(String(name || ''))
    if (!safe) { response.status(400).json({ code: 'INVALID_FILE', message: 'Dosya adı geçersiz.' }); return }
    const file = join(root, safe)
    let info
    try { info = await stat(file); if (!info.isFile()) throw new Error('yok') }
    catch { response.status(404).json({ code: 'NOT_FOUND', message: 'Dosya bulunamadı.' }); return }

    response.setHeader('Content-Type', safe.endsWith('.yml') ? 'text/yaml; charset=utf-8' : 'application/octet-stream')
    response.setHeader('Accept-Ranges', 'bytes')
    response.setHeader('Cache-Control', 'no-store')
    const range = /^bytes=(\d*)-(\d*)$/.exec(String(request.headers?.range || ''))
    if (range) {
      const start = range[1] ? Number(range[1]) : Math.max(0, info.size - Number(range[2] || 0))
      const end = range[1] && range[2] ? Number(range[2]) : info.size - 1
      if (!Number.isFinite(start) || start >= info.size || end < start) {
        response.setHeader('Content-Range', `bytes */${info.size}`)
        response.status(416).end()
        return
      }
      const last = Math.min(end, info.size - 1)
      response.status(206)
      response.setHeader('Content-Range', `bytes ${start}-${last}/${info.size}`)
      response.setHeader('Content-Length', last - start + 1)
      createReadStream(file, { start, end: last }).pipe(response)
      return
    }
    response.setHeader('Content-Length', info.size)
    createReadStream(file).pipe(response)
  }
}
