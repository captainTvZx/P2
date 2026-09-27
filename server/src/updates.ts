import { Controller, Get, Inject, OnModuleDestroy, OnModuleInit, Param, Req, Res } from '@nestjs/common'
import { createReadStream } from 'node:fs'
import { readFile, stat } from 'node:fs/promises'
import { join } from 'node:path'
import { RealtimeGateway } from './realtime'

// Otomatik güncelleme dosyaları: sunucuyu barındıran kişi yeni kurulum
// dosyasını buraya koyar (npm run publish:update), ekipteki uygulamalar
// açılışta buraya bakıp kendini günceller. Uç bilerek public: istemci daha
// giriş yapmadan da güncelleyebilmeli.
// Güncelleme dosyalarının yeri: normalde uygulama süreci P2_UPDATE_DIR'i verir.
// Sunucu servis olarak (ya da geliştirme modunda) bu değişken olmadan kalkarsa
// dosyaları yine de bulsun diye yayınlama dizini de sırayla denenir — aksi hâlde
// latest.yml sessizce 404 döner ve kimse güncellenemez.
const roots = process.env.P2_UPDATE_DIR ? [process.env.P2_UPDATE_DIR] : [
  process.platform === 'win32' ? join(process.env.ProgramData || 'C:\\ProgramData', 'P2', 'updates') : '',
  join(process.cwd(), '.p2-data', 'updates'),
].filter(Boolean)
// İlk bulunan kopya sunulur; hiçbirinde yoksa 404.
async function locate(name: string) {
  for (const dir of roots) {
    const file = join(dir, name)
    try { const info = await stat(file); if (info.isFile()) return { file, info } } catch {}
  }
  return null
}
// Dizin dışına çıkma denemeleri: yalnızca düz dosya adları kabul edilir.
const safeFile = (value: string) => /^[A-Za-z0-9._-]+$/.test(value) && !value.startsWith('.') ? value : ''

// Yayınlanan sürümü okur: latest.yml'in ilk satırı "version: 0.5.8".
async function publishedVersion() {
  const found = await locate('latest.yml')
  if (!found) return ''
  try { return (/^version:\s*(.+)$/m.exec(await readFile(found.file, 'utf8'))?.[1] || '').trim() } catch { return '' }
}

@Controller('updates')
export class UpdatesController implements OnModuleInit, OnModuleDestroy {
  // Yayın klasörü sürekli izlenir: yeni sürüm kopyalandığı anda o sırada bağlı
  // olan herkese haber gider, kimse bir sonraki denetimi beklemez.
  private timer?: ReturnType<typeof setInterval>
  private current = ''
  constructor(@Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  async onModuleInit() {
    this.current = await publishedVersion()
    this.timer = setInterval(() => { void this.watch() }, 5000)
    this.timer.unref?.()
  }
  onModuleDestroy() { if (this.timer) clearInterval(this.timer) }

  private async watch() {
    const version = await publishedVersion()
    if (!version || version === this.current) return
    this.current = version
    this.stream.broadcast('update:published', { version })
  }

  @Get('latest.yml')
  async latest(@Res() response: any) {
    const found = await locate('latest.yml')
    try {
      if (!found) throw new Error('yok')
      const { file, info } = found
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
    const found = await locate(safe)
    if (!found) { response.status(404).json({ code: 'NOT_FOUND', message: 'Dosya bulunamadı.' }); return }
    const { file, info } = found

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
