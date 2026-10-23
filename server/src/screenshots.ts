import { BadRequestException, Controller, Delete, Get, Inject, NotFoundException, Param, Patch, Post, Query, Req, UseGuards } from '@nestjs/common'
import { IsInt, IsOptional, IsString, Length, Min } from 'class-validator'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { effectivePermissions, requireProjectAccess } from './permissions'
import { pageArgs } from './paging'
import { uploadRoot, type UploadMeta } from './uploads'
import { canManageScreenshot, clampDimension, normalizeCaption, normalizeTakenAt, uploadFileId, uploadOwnedImage, validUploadPath } from './screenshotRules'

// Proje ekran görüntüsü galerisi: overlay "Ekran görüntüsü" penceresi ve Kütüphane "Ekran görüntüleri" sekmesi.
// Yükleme iki adımlı: istemci POST /uploads (JPEG, base64) → buraya yalnız uploadPath + boyutlar.
// Sunucu yolu doğrular, "<id>.json" meta dosyasından sahibi (ben) ve türü (görsel) denetler.
// Proje izleyicisi listeler; sahibi ya da projeyi yöneten (channel.manage vekili) düzenler/siler (yalnız satır).

export class CreateScreenshotDto {
  @IsString() uploadPath!: string
  @IsInt() @Min(1) width!: number
  @IsInt() @Min(1) height!: number
  @IsOptional() @IsInt() @Min(0) size?: number
  @IsOptional() @IsString() @Length(0, 500) caption?: string
  @IsOptional() @IsString() releaseId?: string
  @IsOptional() @IsString() takenAt?: string
}
export class UpdateScreenshotDto {
  @IsOptional() @IsString() @Length(0, 500) caption?: string
}

const userSelect = { select: { id: true, username: true, displayName: true, avatarUrl: true } }

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects/:projectId/screenshots')
export class ScreenshotsController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async access(organizationId: string, projectId: string, userId: string) {
    await requireProjectAccess(this.db, organizationId, userId, projectId)
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId, deletedAt: null }, select: { id: true } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    const effective = await effectivePermissions(this.db, organizationId, userId)
    return { canManage: effective.permissions.includes('channel.manage' as any) }
  }

  private async find(projectId: string, id: string) {
    const shot = await this.db.projectScreenshot.findFirst({ where: { id, projectId }, include: { user: userSelect } })
    if (!shot) throw new NotFoundException({ code: 'SCREENSHOT_NOT_FOUND', message: 'Ekran görüntüsü bulunamadı.' })
    return shot
  }

  private async readMeta(uploadPath: string): Promise<UploadMeta | null> {
    try { return JSON.parse(await readFile(join(uploadRoot, `${uploadFileId(uploadPath)}.json`), 'utf8')) as UploadMeta }
    catch { return null }
  }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Query('before') before?: string, @Query('limit') limit?: string, @Query('userId') userId?: string) {
    const me = req.user.sub as string
    await this.access(organizationId, projectId, me)
    const page = pageArgs(before, limit, { take: 60, max: 200 })
    const cursor = 'createdAt' in page.where ? { takenAt: page.where.createdAt } : {}
    return this.db.projectScreenshot.findMany({
      where: { projectId, ...(userId ? { userId: String(userId) } : {}), ...cursor },
      orderBy: [{ takenAt: 'desc' }, { createdAt: 'desc' }],
      take: page.take,
      include: { user: userSelect },
    })
  }

  @Post()
  async create(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Dto(CreateScreenshotDto) dto: CreateScreenshotDto) {
    const me = req.user.sub as string
    await this.access(organizationId, projectId, me)
    if (!validUploadPath(dto.uploadPath)) throw new BadRequestException({ code: 'SCREENSHOT_PATH', message: 'Görüntü yolu geçersiz.' })
    const width = clampDimension(dto.width), height = clampDimension(dto.height)
    if (!width || !height) throw new BadRequestException({ code: 'SCREENSHOT_SIZE', message: 'Görüntü boyutu geçersiz (en çok 7680 px).' })
    const meta = await this.readMeta(dto.uploadPath)
    if (!meta || !uploadOwnedImage(meta, me)) throw new BadRequestException({ code: 'SCREENSHOT_UPLOAD', message: 'Yüklenen dosya bulunamadı ya da sana ait bir görsel değil.' })
    let releaseId: string | null = null
    if (dto.releaseId) {
      const release = await this.db.release.findFirst({ where: { id: dto.releaseId, projectId }, select: { id: true } })
      releaseId = release?.id || null
    }
    const shot = await this.db.projectScreenshot.create({
      data: {
        projectId, userId: me, uploadPath: dto.uploadPath, width, height,
        size: Math.max(0, Math.floor(Number(meta.size) || Number(dto.size) || 0)),
        caption: normalizeCaption(dto.caption), releaseId, takenAt: normalizeTakenAt(dto.takenAt),
      },
      include: { user: userSelect },
    })
    void this.stream.emitProject(organizationId, projectId, 'screenshot:added', { projectId, screenshot: shot })
    return shot
  }

  @Patch(':id')
  async update(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('id') id: string, @Req() req: any, @Dto(UpdateScreenshotDto) dto: UpdateScreenshotDto) {
    const me = req.user.sub as string
    const { canManage } = await this.access(organizationId, projectId, me)
    const current = await this.find(projectId, id)
    if (!canManageScreenshot(current, me, canManage)) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Bu görüntüyü düzenleyemezsin.' })
    const shot = await this.db.projectScreenshot.update({ where: { id }, data: { caption: dto.caption !== undefined ? normalizeCaption(dto.caption) : current.caption }, include: { user: userSelect } })
    void this.stream.emitProject(organizationId, projectId, 'screenshot:updated', { projectId, screenshot: shot })
    return shot
  }

  @Delete(':id')
  async remove(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('id') id: string, @Req() req: any) {
    const me = req.user.sub as string
    const { canManage } = await this.access(organizationId, projectId, me)
    const current = await this.find(projectId, id)
    if (!canManageScreenshot(current, me, canManage)) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Bu görüntüyü silemezsin.' })
    await this.db.projectScreenshot.delete({ where: { id } })
    void this.stream.emitProject(organizationId, projectId, 'screenshot:deleted', { projectId, id })
    return { ok: true }
  }
}
