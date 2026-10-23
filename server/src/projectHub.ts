import { Body, Controller, Delete, Get, Inject, NotFoundException, Param, Patch, Post, Put, Req, UseGuards } from '@nestjs/common'
import { IsBoolean, IsOptional, IsString, Length } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { effectivePermissions, requireProjectAccess } from './permissions'

// Kütüphane vitrini ("hub"): elle yazılan aktivite gönderileri, başarımlar,
// incelemeler ve oynama süresi. Sürüm/parça işleri releases.ts'te kalır.

export class PostDto { @IsString() @Length(1, 4000) content!: string }
export class AchievementDto {
  @IsOptional() @IsString() @Length(1, 60) title?: string
  @IsOptional() @IsString() @Length(0, 240) description?: string
  @IsOptional() @IsString() @Length(1, 8) icon?: string
}
export class UnlockDto { @IsBoolean() unlocked!: boolean }
export class ReviewDto {
  @IsBoolean() recommended!: boolean
  @IsOptional() @IsString() @Length(0, 2000) body?: string
}

const userSelect = { select: { id: true, username: true, displayName: true, avatarUrl: true } }

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects/:projectId/hub')
export class ProjectHubController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async access(organizationId: string, projectId: string, userId: string) {
    await requireProjectAccess(this.db, organizationId, userId, projectId)
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId, deletedAt: null }, select: { id: true, name: true } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    const effective = await effectivePermissions(this.db, organizationId, userId)
    const has = (id: string) => effective.permissions.includes(id as any)
    const manage = has('channel.manage'), publish = has('release.publish')
    // Gönderi: sürüm yayınlayan ya da projeyi yöneten; başarım tanımı: yönetici; inceleme/kazanım: herkes.
    return { project, perms: { post: manage || publish, manage, review: true } }
  }
  private changed(organizationId: string, projectId: string, what: string) {
    void this.stream.emitProject(organizationId, projectId, 'hub:changed', { projectId, what })
  }

  @Get()
  async overview(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const { perms } = await this.access(organizationId, projectId, userId)
    const [posts, achievements, reviews, mine, team] = await Promise.all([
      this.db.projectPost.findMany({ where: { projectId }, orderBy: { createdAt: 'desc' }, take: 60, include: { author: userSelect } }),
      this.db.projectAchievement.findMany({ where: { projectId }, orderBy: { createdAt: 'asc' }, include: { unlocks: { orderBy: { unlockedAt: 'desc' }, include: { user: userSelect } } } }),
      this.db.projectReview.findMany({ where: { projectId }, orderBy: { updatedAt: 'desc' }, include: { user: userSelect } }),
      this.db.releaseInstall.aggregate({ where: { projectId, userId }, _sum: { playSeconds: true }, _max: { lastLaunchedAt: true } }),
      this.db.releaseInstall.groupBy({ by: ['userId'], where: { projectId, playSeconds: { gt: 0 } }, _sum: { playSeconds: true } }),
    ])
    const playtime = {
      mine: mine._sum.playSeconds || 0,
      lastPlayedAt: mine._max.lastLaunchedAt,
      team: team.reduce((sum, row) => sum + (row._sum.playSeconds || 0), 0),
      players: team.length,
    }
    return {
      posts,
      achievements: achievements.map(row => ({ ...row, unlockedByMe: row.unlocks.some(u => u.userId === userId) })),
      reviews,
      playtime,
      permissions: perms,
    }
  }

  // ---- aktivite gönderileri ----
  @Post('posts')
  async createPost(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Dto(PostDto) dto: PostDto) {
    const userId = req.user.sub as string
    const { perms } = await this.access(organizationId, projectId, userId)
    if (!perms.post) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Aktivite yazma yetkin yok.' })
    const post = await this.db.projectPost.create({ data: { projectId, authorId: userId, content: dto.content.trim() }, include: { author: userSelect } })
    this.changed(organizationId, projectId, 'posts')
    return post
  }

  @Patch('posts/:postId')
  async updatePost(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('postId') postId: string, @Req() req: any, @Dto(PostDto) dto: PostDto) {
    const userId = req.user.sub as string
    const { perms } = await this.access(organizationId, projectId, userId)
    const post = await this.db.projectPost.findFirst({ where: { id: postId, projectId }, select: { authorId: true } })
    if (!post) throw new NotFoundException({ code: 'POST_NOT_FOUND', message: 'Gönderi bulunamadı.' })
    if (post.authorId !== userId && !perms.manage) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Bu gönderiyi düzenleyemezsin.' })
    const updated = await this.db.projectPost.update({ where: { id: postId }, data: { content: dto.content.trim() }, include: { author: userSelect } })
    this.changed(organizationId, projectId, 'posts')
    return updated
  }

  @Delete('posts/:postId')
  async deletePost(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('postId') postId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const { perms } = await this.access(organizationId, projectId, userId)
    const post = await this.db.projectPost.findFirst({ where: { id: postId, projectId }, select: { authorId: true } })
    if (!post) throw new NotFoundException({ code: 'POST_NOT_FOUND', message: 'Gönderi bulunamadı.' })
    if (post.authorId !== userId && !perms.manage) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Bu gönderiyi silemezsin.' })
    await this.db.projectPost.delete({ where: { id: postId } })
    this.changed(organizationId, projectId, 'posts')
    return { ok: true }
  }

  // ---- başarımlar ----
  @Post('achievements')
  async createAchievement(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Dto(AchievementDto) dto: AchievementDto) {
    const userId = req.user.sub as string
    const { perms } = await this.access(organizationId, projectId, userId)
    if (!perms.manage) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Başarım tanımlama yetkin yok.' })
    const title = (dto.title || '').trim()
    if (!title) throw new NotFoundException({ code: 'ACHIEVEMENT_TITLE', message: 'Başarım adı gerekli.' })
    const row = await this.db.projectAchievement.create({ data: { projectId, createdById: userId, title, description: dto.description?.trim() || null, icon: (dto.icon || '🏆').trim() || '🏆' }, include: { unlocks: { include: { user: userSelect } } } })
    this.changed(organizationId, projectId, 'achievements')
    return { ...row, unlockedByMe: false }
  }

  @Patch('achievements/:achievementId')
  async updateAchievement(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('achievementId') achievementId: string, @Req() req: any, @Dto(AchievementDto) dto: AchievementDto) {
    const userId = req.user.sub as string
    const { perms } = await this.access(organizationId, projectId, userId)
    if (!perms.manage) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Başarım düzenleme yetkin yok.' })
    const exists = await this.db.projectAchievement.findFirst({ where: { id: achievementId, projectId }, select: { id: true } })
    if (!exists) throw new NotFoundException({ code: 'ACHIEVEMENT_NOT_FOUND', message: 'Başarım bulunamadı.' })
    const data: any = {}
    if (dto.title !== undefined && dto.title.trim()) data.title = dto.title.trim()
    if (dto.description !== undefined) data.description = dto.description.trim() || null
    if (dto.icon !== undefined && dto.icon.trim()) data.icon = dto.icon.trim()
    const row = await this.db.projectAchievement.update({ where: { id: achievementId }, data, include: { unlocks: { include: { user: userSelect } } } })
    this.changed(organizationId, projectId, 'achievements')
    return { ...row, unlockedByMe: row.unlocks.some(u => u.userId === userId) }
  }

  @Delete('achievements/:achievementId')
  async deleteAchievement(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('achievementId') achievementId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const { perms } = await this.access(organizationId, projectId, userId)
    if (!perms.manage) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Başarım silme yetkin yok.' })
    const result = await this.db.projectAchievement.deleteMany({ where: { id: achievementId, projectId } })
    if (!result.count) throw new NotFoundException({ code: 'ACHIEVEMENT_NOT_FOUND', message: 'Başarım bulunamadı.' })
    this.changed(organizationId, projectId, 'achievements')
    return { ok: true }
  }

  // Kişi kendi kazanımını işaretler/kaldırır.
  @Post('achievements/:achievementId/unlock')
  async unlock(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('achievementId') achievementId: string, @Req() req: any, @Dto(UnlockDto) dto: UnlockDto) {
    const userId = req.user.sub as string
    await this.access(organizationId, projectId, userId)
    const exists = await this.db.projectAchievement.findFirst({ where: { id: achievementId, projectId }, select: { id: true } })
    if (!exists) throw new NotFoundException({ code: 'ACHIEVEMENT_NOT_FOUND', message: 'Başarım bulunamadı.' })
    if (dto.unlocked) await this.db.achievementUnlock.upsert({ where: { achievementId_userId: { achievementId, userId } }, create: { achievementId, userId }, update: {} })
    else await this.db.achievementUnlock.deleteMany({ where: { achievementId, userId } })
    this.changed(organizationId, projectId, 'achievements')
    return { ok: true, unlocked: dto.unlocked }
  }

  // ---- incelemeler (kişi başına bir tane) ----
  @Put('reviews')
  async review(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Dto(ReviewDto) dto: ReviewDto) {
    const userId = req.user.sub as string
    await this.access(organizationId, projectId, userId)
    const body = dto.body?.trim() || null
    const row = await this.db.projectReview.upsert({
      where: { projectId_userId: { projectId, userId } },
      create: { projectId, userId, recommended: dto.recommended, body },
      update: { recommended: dto.recommended, body },
      include: { user: userSelect },
    })
    this.changed(organizationId, projectId, 'reviews')
    return row
  }

  @Delete('reviews')
  async deleteReview(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.access(organizationId, projectId, userId)
    await this.db.projectReview.deleteMany({ where: { projectId, userId } })
    this.changed(organizationId, projectId, 'reviews')
    return { ok: true }
  }
}
