import { BadRequestException, Body, ConflictException, Controller, Delete, Get, Inject, NotFoundException, OnModuleInit, Param, Patch, PayloadTooLargeException, Post, Put, Query, Req, Res, UseGuards } from '@nestjs/common'
import { IsArray, IsBoolean, IsIn, IsInt, IsOptional, IsString, Length, Matches, Max, Min } from 'class-validator'
import { createReadStream } from 'node:fs'
import { stat } from 'node:fs/promises'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { effectivePermissions, projectAudience, requirePermission, requireProjectAccess } from './permissions'
import { recordActivity } from './activity'
import { rangeOf } from './uploads'
import { CHUNK_SIZE, chunkExists, chunkPath, ensureDirs, gcChunks, isHash, missingChunks, scheduleGc, sweepTmp, writeChunk } from './releaseStore'
import { chunkLength, diffManifests, sameManifest, validateManifestInput, type Manifest } from './releaseManifest'

// Kategori (kanal) kullanıcı tanımlıdır: "alpha", "beta", "release", "demo"… Harf/rakamla
// başlar, en çok 24 karakter; boşluklar tekilleştirilir.
export const CHANNEL_RE = /^[\p{L}\p{N}][\p{L}\p{N} ._-]{0,23}$/u
export const normalizeChannel = (value: string) => value.trim().replace(/\s+/g, ' ')
const VERSION_RE = /^[0-9A-Za-z.+-]{1,40}$/

export class CreateReleaseDto {
  @IsString() @Matches(CHANNEL_RE) channel!: string
  @IsString() @Matches(VERSION_RE) version!: string
  @IsOptional() @IsString() @Length(0, 4000) notes?: string
  @IsString() executable!: string
  @IsOptional() @IsString() @Length(0, 500) launchArgs?: string
  @IsOptional() @IsBoolean() overlay?: boolean
  @IsArray() files!: unknown[]
}

export class UpdateReleaseDto {
  @IsOptional() @IsString() @Matches(CHANNEL_RE) channel?: string
  @IsOptional() @IsString() @Matches(VERSION_RE) version?: string
  @IsOptional() @IsString() @Length(0, 4000) notes?: string
  @IsOptional() @IsString() @Length(0, 500) launchArgs?: string
  @IsOptional() @IsBoolean() overlay?: boolean
}

export class MissingDto { @IsOptional() @IsArray() hashes?: unknown[] }

export class InstalledDto {
  @IsString() @Length(1, 64) @Matches(/^[A-Za-z0-9_-]+$/) deviceId!: string
  @IsString() @IsIn(['INSTALLED', 'UPDATING', 'FAILED', 'REMOVED']) status!: 'INSTALLED' | 'UPDATING' | 'FAILED' | 'REMOVED'
  @IsOptional() @IsBoolean() pinned?: boolean
  @IsOptional() @IsString() @Length(0, 40) clientVersion?: string
  @IsOptional() @IsBoolean() launched?: boolean
  // Kapanan oturumun süresi (sn); cihazın toplamına eklenir.
  @IsOptional() @IsInt() @Min(0) @Max(7 * 24 * 3600) playSeconds?: number
}

const authorSelect = { select: { id: true, username: true, displayName: true, avatarUrl: true } }
// Liste ve ayrıntı görünümleri manifesti hiç okumaz; 50 GB'lık bir build'in
// manifesti ~1 MB olabilir.
const releaseSelect = {
  id: true, projectId: true, channel: true, version: true, notes: true, status: true, executable: true, launchArgs: true, platform: true, overlay: true,
  totalSize: true, fileCount: true, chunkCount: true, createdById: true, createdAt: true, updatedAt: true, publishedAt: true, archivedAt: true, liveAt: true,
  createdBy: authorSelect,
  _count: { select: { installs: { where: { status: 'INSTALLED' } } } },
} as const
const diffBaseSelect = { id: true, version: true, channel: true, status: true, publishedAt: true, manifest: true } as const

function view(row: any) {
  const { _count, totalSize, ...rest } = row
  return { ...rest, totalSize: Number(totalSize), installCount: _count?.installs ?? 0 }
}

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects/:projectId/releases')
export class ReleasesController implements OnModuleInit {
  private touched = new Map<string, number>()
  // Parça indirme yetkisi: aynı kişi aynı projeden binlerce parça çeker,
  // her biri için veritabanına gitmemek adına olumlu yanıtlar 5 dk tutulur.
  private chunkAccess = new Map<string, number>()
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  async onModuleInit() {
    try { await ensureDirs(); await sweepTmp() } catch {}
    setTimeout(() => void gcChunks(this.db, true), 30_000).unref?.()
    setInterval(() => void gcChunks(this.db, true), 6 * 3600 * 1000).unref?.()
  }

  private async project(organizationId: string, projectId: string, userId: string) {
    await requireProjectAccess(this.db, organizationId, userId, projectId)
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId, deletedAt: null }, select: { id: true, name: true } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    return project
  }
  private async perms(organizationId: string, userId: string) {
    const effective = await effectivePermissions(this.db, organizationId, userId)
    const has = (id: string) => effective.permissions.includes(id as any)
    return { publish: has('release.publish'), download: has('release.download') }
  }
  private async release(projectId: string, releaseId: string) {
    const row = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: releaseSelect })
    if (!row) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    return row
  }
  // Taslaklar yalnız yayınlayabilenlere görünür; yayınlanmış her kategori herkese açık.
  private visible(row: { status: string }, perms: { publish: boolean }) {
    return row.status !== 'DRAFT' || perms.publish
  }
  private touch(releaseId: string) {
    const last = this.touched.get(releaseId) || 0
    if (Date.now() - last < 60_000) return
    this.touched.set(releaseId, Date.now())
    void this.db.release.updateMany({ where: { id: releaseId, status: 'DRAFT' }, data: { updatedAt: new Date() } }).catch(() => {})
  }

  // ---- liste / ayrıntı ----
  @Get()
  async list(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Query('channel') channel?: string) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    const perms = await this.perms(organizationId, userId)
    if (!perms.download && !perms.publish) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Kütüphaneyi görme yetkin yok.' })
    const rows = await this.db.release.findMany({
      where: { projectId, status: { not: 'ARCHIVED' }, ...(channel ? { channel: normalizeChannel(channel) } : {}) },
      orderBy: [{ publishedAt: { sort: 'desc', nulls: 'first' } }, { createdAt: 'desc' }],
      select: releaseSelect,
    })
    const releases = rows.filter(row => this.visible(row, perms)).map(view)
    // Kategori başına canlı sürüm (yoksa en son yayın); kategori sırası: önce yayınlı olanlar, sonra yalnız taslağı olanlar.
    const latest: Record<string, ReturnType<typeof view> | null> = {}
    for (const row of releases) if (row.status === 'PUBLISHED' && row.liveAt) latest[row.channel] = row
    for (const row of releases) if (row.status === 'PUBLISHED' && !latest[row.channel]) latest[row.channel] = row
    const channels = [...new Set([...releases.filter(row => row.status === 'PUBLISHED').map(row => row.channel), ...releases.map(row => row.channel)])]
    return { releases, latest, channels, permissions: perms }
  }

  @Get('latest')
  async latest(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Query('channel') channel?: string) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    const perms = await this.perms(organizationId, userId)
    if (!perms.download && !perms.publish) return { release: null }
    const where = { projectId, status: 'PUBLISHED', ...(channel ? { channel: normalizeChannel(channel) } : {}) }
    const row = await this.db.release.findFirst({ where: { ...where, liveAt: { not: null } }, orderBy: { liveAt: 'desc' }, select: releaseSelect })
      || await this.db.release.findFirst({ where, orderBy: { publishedAt: 'desc' }, select: releaseSelect })
    return { release: row ? view(row) : null }
  }

  @Get('installs')
  async installs(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    await requirePermission(this.db, organizationId, userId, 'release.publish')
    const rows = await this.db.releaseInstall.findMany({
      where: { projectId, status: { not: 'REMOVED' } }, orderBy: { updatedAt: 'desc' },
      select: { deviceId: true, releaseId: true, channel: true, version: true, status: true, pinned: true, clientVersion: true, installedAt: true, lastLaunchedAt: true, playSeconds: true, updatedAt: true, user: authorSelect },
    })
    return { installs: rows }
  }

  // Parça indirme: aynı parça birçok sürüm/projede olabilir; bu projede yayınlanmış
  // (ya da kendi taslağı olan) bir sürüm başvuruyorsa verilir.
  @Get('chunks/:hash')
  async chunk(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('hash') hash: string, @Req() req: any, @Res() res: any) {
    const userId = req.user.sub as string
    if (!isHash(hash)) return res.status(404).end()
    const key = `${projectId}|${userId}|${hash}`
    const until = this.chunkAccess.get(key) || 0
    if (until < Date.now()) {
      await this.project(organizationId, projectId, userId)
      const perms = await this.perms(organizationId, userId)
      if (!perms.download && !perms.publish) return res.status(403).json({ code: 'PERMISSION_DENIED', message: 'Sürüm indirme yetkin yok.' })
      const ref = await this.db.releaseChunkRef.findFirst({ where: { hash, release: { projectId, OR: [{ status: 'PUBLISHED' }, { createdById: userId }] } }, select: { hash: true } })
      if (!ref) return res.status(404).end()
      if (this.chunkAccess.size > 50_000) this.chunkAccess.clear()
      this.chunkAccess.set(key, Date.now() + 5 * 60_000)
    }
    const path = chunkPath(hash)
    let info
    try { info = await stat(path) } catch { return res.status(404).end() }
    const range = rangeOf(req.headers.range, info.size)
    res.setHeader('Accept-Ranges', 'bytes')
    res.setHeader('Content-Type', 'application/octet-stream')
    res.setHeader('X-Content-Type-Options', 'nosniff')
    res.setHeader('Cache-Control', 'private, max-age=31536000, immutable')
    if (range === false) { res.setHeader('Content-Range', `bytes */${info.size}`); return res.status(416).end() }
    if (range) {
      res.status(206); res.setHeader('Content-Range', `bytes ${range.start}-${range.end}/${info.size}`); res.setHeader('Content-Length', String(range.end - range.start + 1))
      return createReadStream(path, { start: range.start, end: range.end }).pipe(res)
    }
    res.setHeader('Content-Length', String(info.size))
    return createReadStream(path).pipe(res)
  }

  @Get(':releaseId')
  async detail(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    const perms = await this.perms(organizationId, userId)
    const row = await this.release(projectId, releaseId)
    if (!this.visible(row, perms) || (!perms.download && !perms.publish)) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    return view(row)
  }

  @Get(':releaseId/manifest')
  async manifest(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any, @Res() res: any) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    const perms = await this.perms(organizationId, userId)
    const row = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: { status: true, channel: true, createdById: true, manifest: true } })
    if (!row || !this.visible(row, perms) || (!perms.download && !perms.publish)) return res.status(404).json({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    if (row.status === 'DRAFT' && row.createdById !== userId && !perms.publish) return res.status(404).json({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    res.setHeader('Cache-Control', 'private, max-age=31536000, immutable')
    return res.json(row.manifest)
  }

  // Önceki paketle fark: `against` verilmezse aynı kategoride bir önceki yayın,
  // yoksa projedeki son yayın temel alınır. Taslak için kategorinin güncel yayını.
  @Get(':releaseId/diff')
  async diff(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any, @Query('against') against?: string) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    const perms = await this.perms(organizationId, userId)
    if (!perms.download && !perms.publish) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    const row = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: { id: true, status: true, channel: true, publishedAt: true, manifest: true } })
    if (!row || !this.visible(row, perms)) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    const before = row.publishedAt ? { publishedAt: { lt: row.publishedAt } } : {}
    let base = against
      ? await this.db.release.findFirst({ where: { id: against, projectId, status: { not: 'ARCHIVED' } }, select: diffBaseSelect })
      : (await this.db.release.findFirst({ where: { projectId, status: 'PUBLISHED', channel: row.channel, id: { not: row.id }, ...before }, orderBy: { publishedAt: 'desc' }, select: diffBaseSelect })
        || await this.db.release.findFirst({ where: { projectId, status: 'PUBLISHED', id: { not: row.id }, ...before }, orderBy: { publishedAt: 'desc' }, select: diffBaseSelect }))
    if (base && (base.id === row.id || !this.visible(base, perms))) base = null
    const result = diffManifests(base ? (base.manifest as unknown as Manifest) : null, row.manifest as unknown as Manifest)
    return { against: base ? { id: base.id, version: base.version, channel: base.channel, publishedAt: base.publishedAt } : null, ...result }
  }

  // ---- yükleme ----
  @Post()
  async create(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Dto(CreateReleaseDto) dto: CreateReleaseDto) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    await requirePermission(this.db, organizationId, userId, 'release.publish')
    const result = validateManifestInput({ executable: dto.executable, launchArgs: dto.launchArgs, files: dto.files })
    if ('error' in result) throw new BadRequestException({ code: result.code, message: result.error })
    const { manifest, uniqueHashes } = result
    const channel = normalizeChannel(dto.channel)
    const existing = await this.db.release.findUnique({ where: { projectId_channel_version: { projectId, channel, version: dto.version } }, select: { id: true, status: true, createdById: true, manifest: true } })
    if (existing) {
      // Aynı kişi aynı taslağı yeniden gönderiyorsa yüklemeye kaldığı yerden devam eder.
      if (existing.status === 'DRAFT' && existing.createdById === userId && sameManifest(existing.manifest as unknown as Manifest, manifest)) {
        const row = await this.release(projectId, existing.id)
        return { release: view(row), missing: await missingChunks(uniqueHashes), resumed: true }
      }
      throw new ConflictException({ code: 'RELEASE_VERSION_EXISTS', message: `${channel} kategorisinde ${dto.version} sürümü zaten var. Yeni bir sürüm numarası ver.`, field: 'version' })
    }
    const sizes = new Map<string, number>()
    for (const file of manifest.files) file.chunks.forEach((hash, index) => { if (!sizes.has(hash)) sizes.set(hash, chunkLength(file, index)) })
    const created = await this.db.$transaction(async tx => {
      const release = await tx.release.create({
        data: {
          projectId, channel, version: dto.version, notes: dto.notes?.trim() || null, status: 'DRAFT',
          executable: manifest.executable, launchArgs: manifest.launchArgs || null, platform: manifest.platform, overlay: dto.overlay !== false,
          totalSize: BigInt(manifest.totalSize), fileCount: manifest.fileCount, chunkCount: manifest.chunkCount, manifest: manifest as any, createdById: userId,
        },
        select: { id: true },
      })
      // Parça satırları önceden "bekleniyor" olarak açılır: ref'ler sayesinde GC
      // yükleme sürerken parçaları silmez. Diskteki varlık "yüklendi" demektir.
      for (let index = 0; index < uniqueHashes.length; index += 5000) {
        const batch = uniqueHashes.slice(index, index + 5000)
        await tx.releaseChunk.createMany({ data: batch.map(hash => ({ hash, size: sizes.get(hash) || CHUNK_SIZE })), skipDuplicates: true })
        await tx.releaseChunkRef.createMany({ data: batch.map(hash => ({ releaseId: release.id, hash })), skipDuplicates: true })
      }
      return release
    }, { timeout: 120_000 })
    const row = await this.release(projectId, created.id)
    return { release: view(row), missing: await missingChunks(uniqueHashes), resumed: false }
  }

  @Post(':releaseId/missing')
  async missing(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any, @Dto(MissingDto) dto: MissingDto) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    await requirePermission(this.db, organizationId, userId, 'release.publish')
    const row = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: { status: true } })
    if (!row) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    const refs = await this.db.releaseChunkRef.findMany({ where: { releaseId }, select: { hash: true } })
    const all = refs.map(ref => ref.hash)
    const wanted = Array.isArray(dto.hashes) && dto.hashes.length ? all.filter(hash => (dto.hashes as unknown[]).includes(hash)) : all
    return { missing: await missingChunks(wanted), total: all.length }
  }

  // Gövde ham ikili veri; JSON ayrıştırıcı octet-stream'e dokunmadığı için
  // istek burada tüketilmemiş bir akış olarak gelir ve doğrudan diske yazılır.
  @Put(':releaseId/chunks/:hash')
  async putChunk(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Param('hash') hash: string, @Req() req: any) {
    const userId = req.user.sub as string
    const drain = () => { try { req.resume() } catch {} }
    if (!isHash(hash)) { drain(); throw new BadRequestException({ code: 'CHUNK_HASH', message: 'Parça özeti geçersiz.' }) }
    if (!/^application\/octet-stream/.test(String(req.headers['content-type'] || ''))) { drain(); throw new BadRequestException({ code: 'CHUNK_CONTENT_TYPE', message: 'Parça ham ikili veri olarak gönderilmeli.' }) }
    const declared = Number(req.headers['content-length'])
    if (Number.isFinite(declared) && declared > CHUNK_SIZE) { drain(); throw new PayloadTooLargeException({ code: 'CHUNK_TOO_LARGE', message: 'Parça 4 MB sınırını aşıyor.' }) }
    try {
      await this.project(organizationId, projectId, userId)
      await requirePermission(this.db, organizationId, userId, 'release.publish')
    } catch (error) { drain(); throw error }
    const row = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: { status: true } })
    if (!row) { drain(); throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' }) }
    if (row.status !== 'DRAFT') { drain(); throw new ConflictException({ code: 'RELEASE_NOT_DRAFT', message: 'Yayınlanmış sürüme parça eklenemez.' }) }
    const ref = await this.db.releaseChunkRef.findUnique({ where: { releaseId_hash: { releaseId, hash } }, select: { hash: true } })
    if (!ref) { drain(); throw new NotFoundException({ code: 'CHUNK_NOT_IN_MANIFEST', message: 'Bu parça sürümün manifestinde yok.' }) }
    if (await chunkExists(hash)) { drain(); this.touch(releaseId); return { ok: true, hash, existed: true } }
    const { size } = await writeChunk(req, hash)
    this.touch(releaseId)
    return { ok: true, hash, size }
  }

  @Post(':releaseId/publish')
  async publish(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const project = await this.project(organizationId, projectId, userId)
    await requirePermission(this.db, organizationId, userId, 'release.publish')
    const refs = await this.db.releaseChunkRef.findMany({ where: { releaseId, release: { projectId } }, select: { hash: true } })
    const missing = await missingChunks(refs.map(ref => ref.hash))
    if (missing.length) throw new ConflictException({ code: 'RELEASE_INCOMPLETE', message: `${missing.length} parça henüz yüklenmedi.`, missing: missing.length })
    const updated = await this.db.release.updateMany({ where: { id: releaseId, projectId, status: 'DRAFT' }, data: { status: 'PUBLISHED', publishedAt: new Date() } })
    if (!updated.count) throw new ConflictException({ code: 'RELEASE_NOT_DRAFT', message: 'Sürüm zaten yayınlanmış.' })
    let row = view(await this.release(projectId, releaseId))
    // Kategorinin canlı sürümü yoksa ilk yayın kendiliğinden canlıya alınır; varsa
    // yayın "seçilebilir" olarak durur, yönetici dilediğini canlıya alır.
    const hasLive = await this.db.release.findFirst({ where: { projectId, channel: row.channel, status: 'PUBLISHED', liveAt: { not: null } }, select: { id: true } })
    void this.stream.emitProject(organizationId, projectId, 'release:published', { projectId, projectName: project.name, release: row })
    if (!hasLive) row = await this.makeLive(organizationId, project, releaseId, userId)
    else await this.notify(organizationId, projectId, userId, `${project.name} ${row.version} (${row.channel}) yayınlandı`)
    // Sürüm notları Kütüphane aktivitesine kendiliğinden düşer (Steam'deki güncelleme duyurusu gibi).
    await this.releasePost(organizationId, projectId, userId, releaseId, `🚀 **${row.version}** · ${row.channel} ${hasLive ? 'yayınlandı' : 'yayınlandı ve canlıya alındı'}${row.notes ? '\n\n' + row.notes : ''}`)
    return row
  }

  private async releasePost(organizationId: string, projectId: string, authorId: string, releaseId: string, content: string) {
    try {
      await this.db.projectPost.create({ data: { projectId, authorId, kind: 'RELEASE', releaseId, content: content.slice(0, 4000) } })
      void this.stream.emitProject(organizationId, projectId, 'hub:changed', { projectId, what: 'posts' })
    } catch {}
  }

  // Canlı sürüm: kategoride herkese giden (otomatik güncellenen) sürüm. Eskiye dönüş de
  // aynı yoldan: eski yayını canlıya al, kurulu oyunlar ona iner.
  @Post(':releaseId/live')
  async live(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const project = await this.project(organizationId, projectId, userId)
    await requirePermission(this.db, organizationId, userId, 'release.publish')
    const row = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: { status: true, liveAt: true } })
    if (!row) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    if (row.status !== 'PUBLISHED') throw new ConflictException({ code: 'RELEASE_NOT_PUBLISHED', message: 'Önce sürümü yayınla.' })
    if (row.liveAt) return view(await this.release(projectId, releaseId))
    const made = await this.makeLive(organizationId, project, releaseId, userId)
    await this.releasePost(organizationId, projectId, userId, releaseId, `📡 **${made.version}** · ${made.channel} canlıya alındı — kurulu oyunlar bu sürüme güncelleniyor.`)
    return made
  }

  private async makeLive(organizationId: string, project: { id: string; name: string }, releaseId: string, actorId: string) {
    const target = await this.db.release.findFirst({ where: { id: releaseId, projectId: project.id }, select: { channel: true } })
    if (!target) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    await this.db.$transaction([
      this.db.release.updateMany({ where: { projectId: project.id, channel: target.channel, liveAt: { not: null } }, data: { liveAt: null } }),
      this.db.release.update({ where: { id: releaseId }, data: { liveAt: new Date() } }),
    ])
    const row = view(await this.release(project.id, releaseId))
    void this.stream.emitProject(organizationId, project.id, 'release:live', { projectId: project.id, projectName: project.name, release: row })
    await this.notify(organizationId, project.id, actorId, `${project.name} ${row.version} (${row.channel}) canlıya alındı`)
    return row
  }

  // Projeyi görebilen herkese zil kaydı düşer (yapan hariç).
  private async notify(organizationId: string, projectId: string, actorId: string, preview: string) {
    try {
      const members = await this.db.organizationMember.findMany({ where: { organizationId }, select: { userId: true } })
      const audience = await projectAudience(this.db, organizationId, projectId, members.map(member => member.userId))
      await recordActivity(this.db, this.stream, audience.map(target => ({ organizationId, userId: target, actorId, kind: 'RELEASE' as const, preview, projectId })))
    } catch {}
  }

  // Taslakta kategori ve sürüm numarası da değişir; yayınlanmış sürümde yalnız notlar,
  // başlatma argümanları ve overlay.
  @Patch(':releaseId')
  async update(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any, @Dto(UpdateReleaseDto) dto: UpdateReleaseDto) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    await requirePermission(this.db, organizationId, userId, 'release.publish')
    const current = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: { status: true, channel: true, version: true, manifest: true } })
    if (!current) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    const data: any = {}
    if (dto.notes !== undefined) data.notes = dto.notes.trim() || null
    if (dto.overlay !== undefined) data.overlay = dto.overlay
    if (dto.launchArgs !== undefined) { data.launchArgs = dto.launchArgs.trim() || null; data.manifest = { ...(current.manifest as any), launchArgs: data.launchArgs || undefined } }
    const nextChannel = dto.channel !== undefined ? normalizeChannel(dto.channel) : current.channel
    const nextVersion = dto.version !== undefined ? dto.version : current.version
    if (nextChannel !== current.channel || nextVersion !== current.version) {
      if (current.status !== 'DRAFT') throw new ConflictException({ code: 'RELEASE_NOT_DRAFT', message: 'Yayınlanmış sürümün numarası ve kategorisi değiştirilemez.' })
      if (await this.db.release.findUnique({ where: { projectId_channel_version: { projectId, channel: nextChannel, version: nextVersion } }, select: { id: true } })) throw new ConflictException({ code: 'RELEASE_VERSION_EXISTS', message: 'Bu kategoride bu sürüm numarası kullanılıyor.', field: 'version' })
      data.channel = nextChannel
      data.version = nextVersion
    }
    await this.db.release.update({ where: { id: releaseId }, data })
    const row = view(await this.release(projectId, releaseId))
    if (row.status === 'PUBLISHED') void this.stream.emitProject(organizationId, projectId, 'release:updated', { projectId, release: row })
    return row
  }

  @Delete(':releaseId')
  async remove(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    await requirePermission(this.db, organizationId, userId, 'release.publish')
    const row = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: { id: true, channel: true, version: true, status: true, liveAt: true } })
    if (!row) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    await this.db.release.delete({ where: { id: releaseId } })
    if (row.status === 'PUBLISHED') void this.stream.emitProject(organizationId, projectId, 'release:deleted', { projectId, releaseId, channel: row.channel, version: row.version })
    // Canlı sürüm silindiyse kategorideki en yeni yayın canlıya geçer.
    if (row.liveAt) {
      const next = await this.db.release.findFirst({ where: { projectId, channel: row.channel, status: 'PUBLISHED' }, orderBy: { publishedAt: 'desc' }, select: { id: true } })
      if (next) { const project = await this.db.project.findUnique({ where: { id: projectId }, select: { id: true, name: true } }); if (project) await this.makeLive(organizationId, project, next.id, userId) }
    }
    scheduleGc(this.db)
    return { ok: true }
  }

  // ---- kurulum kayıtları ----
  @Post(':releaseId/installed')
  async installed(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('releaseId') releaseId: string, @Req() req: any, @Dto(InstalledDto) dto: InstalledDto) {
    const userId = req.user.sub as string
    await this.project(organizationId, projectId, userId)
    const perms = await this.perms(organizationId, userId)
    if (!perms.download && !perms.publish) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Sürüm indirme yetkin yok.' })
    const row = await this.db.release.findFirst({ where: { id: releaseId, projectId }, select: { id: true, channel: true, version: true } })
    if (!row) throw new NotFoundException({ code: 'RELEASE_NOT_FOUND', message: 'Sürüm bulunamadı.' })
    const now = new Date()
    const base = { releaseId: row.id, channel: row.channel, version: row.version, status: dto.status, ...(dto.pinned === undefined ? {} : { pinned: dto.pinned }), ...(dto.clientVersion ? { clientVersion: dto.clientVersion } : {}), ...(dto.launched ? { lastLaunchedAt: now } : {}) }
    const played = dto.playSeconds || 0
    const saved = await this.db.releaseInstall.upsert({
      where: { projectId_userId_deviceId: { projectId, userId, deviceId: dto.deviceId } },
      create: { projectId, userId, deviceId: dto.deviceId, pinned: dto.pinned ?? false, installedAt: now, playSeconds: played, ...base },
      update: { ...base, ...(dto.status === 'INSTALLED' ? { installedAt: now } : {}), ...(played ? { playSeconds: { increment: played } } : {}) },
      select: { deviceId: true, releaseId: true, channel: true, version: true, status: true, pinned: true, installedAt: true, lastLaunchedAt: true, playSeconds: true },
    })
    if (played || dto.launched) void this.stream.emitProject(organizationId, projectId, 'hub:changed', { projectId, what: 'playtime' })
    return saved
  }
}
