import { Body, ConflictException, Controller, Delete, Get, Inject, NotFoundException, Param, Patch, Post, Query, Req, UseGuards } from '@nestjs/common'
import { IsIn, IsOptional, IsString, Length, Matches, IsBoolean } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { mentionedUserIds, recordActivity } from './activity'
import { channelAudience, channelPermissions, requireChannelPermission, requirePermission, requireProjectAccess, visibleChannelIds } from './permissions'
import { pageArgs } from './paging'

export class CreateProjectDto {
  @IsString() @Length(2, 60) name!: string
  @IsString() @Length(2, 40) @Matches(/^[a-z0-9-]+$/) slug!: string
  @IsOptional() @IsString() @Length(0, 240) description?: string
  @IsOptional() @IsString() @IsIn(['UNREAL', 'UNITY', 'GODOT', 'CUSTOM']) engine?: string
}

export class UpdateProjectDto {
  // null: başlangıç kanalı kaldırılır; metin: o kanal proje açılınca ilk gelir.
  @IsOptional() defaultChannelId?: string | null
  // Kütüphane vitrini için kapak (16:9) ve ray avatarı için ikon; /uploads adresi ya da null.
  @IsOptional() coverUrl?: string | null
  @IsOptional() iconUrl?: string | null
  // Kütüphane vitrinindeki "Proje haberleri" metni (zengin metin, 4000 karakter).
  @IsOptional() @IsString() @Length(0, 4000) description?: string
  // Kütüphanede uygulama olarak görünsün mü (release.publish yetkisi ister).
  @IsOptional() @IsBoolean() libraryEnabled?: boolean
}

export class CreateChannelDto {
  @IsString() categoryId!: string
  @IsString() @Length(1, 40) name!: string
  @IsString() @IsIn(['TEXT', 'ANNOUNCEMENT', 'VOICE', 'FORUM']) type!: 'TEXT' | 'ANNOUNCEMENT' | 'VOICE' | 'FORUM'
}

export class UpdateChannelDto {
  @IsOptional() @IsString() @Length(1, 40) name?: string
  @IsOptional() @IsString() @Length(0, 240) topic?: string
}

export class CreateMessageDto {
  @IsString() @Length(1, 4000) content!: string
  @IsOptional() @IsString() quotedMessageId?: string
}

export class EditMessageDto { @IsString() @Length(1, 4000) content!: string }
export class ForwardMessageDto { @IsString() targetChannelId!: string }
export class ReactMessageDto { @IsString() @Length(1, 16) @Matches(/^\S+$/) emoji!: string }

type ReactionRow = { emoji: string; userId: string }
export function groupReactions(rows: ReactionRow[], userId: string) {
  const summary = new Map<string, { emoji: string; count: number; reacted: boolean }>()
  for (const row of rows) {
    const item = summary.get(row.emoji) ?? { emoji: row.emoji, count: 0, reacted: false }
    item.count += 1
    if (row.userId === userId) item.reacted = true
    summary.set(row.emoji, item)
  }
  return [...summary.values()]
}

export class CreateCategoryDto {
  @IsString() @Length(1, 32) name!: string
}

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects')
export class ProjectsController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}
  private async membership(organizationId: string, userId: string) {
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
    return member
  }
  @Get()
  async list(@Param('organizationId') organizationId: string, @Req() req: any) {
    const member = await this.membership(organizationId, req.user.sub)
    return this.db.project.findMany({
      where: { organizationId, deletedAt: null, ...(member.role === 'OWNER' ? {} : { OR: [{ ownerId: req.user.sub }, { members: { some: { userId: req.user.sub } } }] }) },
      orderBy: { updatedAt: 'desc' },
      select: { id: true, name: true, slug: true, description: true, engine: true, ownerId: true, defaultChannelId: true, coverUrl: true, iconUrl: true, libraryEnabled: true, createdAt: true, updatedAt: true },
    })
  }

  // Sol raydaki proje rozetleri: kişinin görebildiği (ve ayrılmadığı) kanallarda
  // son okumadan sonra başkalarının yazdığı mesaj sayısı, proje başına toplanır.
  @Get('unread')
  async unread(@Param('organizationId') organizationId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const member = await this.membership(organizationId, userId)
    const projects = await this.db.project.findMany({
      where: { organizationId, deletedAt: null, ...(member.role === 'OWNER' ? {} : { OR: [{ ownerId: userId }, { members: { some: { userId } } }] }) },
      select: { id: true, channelCategories: { where: { deletedAt: null }, select: { channels: { where: { deletedAt: null }, select: { id: true } } } } },
    })
    const channelProject = new Map<string, string>()
    for (const project of projects) for (const category of project.channelCategories) for (const channel of category.channels) channelProject.set(channel.id, project.id)
    const ids = [...channelProject.keys()]
    if (!ids.length) return {}
    const [visible, hidden, reads] = await Promise.all([
      visibleChannelIds(this.db, organizationId, userId, ids),
      this.db.userPreference.findMany({ where: { userId, organizationId, kind: 'CHANNEL', hidden: true }, select: { targetId: true } }),
      this.db.channelRead.findMany({ where: { userId, channelId: { in: ids } }, select: { channelId: true, lastReadAt: true } }),
    ])
    const skip = new Set(hidden.map(row => row.targetId))
    const lastRead = new Map(reads.map(row => [row.channelId, row.lastReadAt]))
    const counted = ids.filter(id => visible.has(id) && !skip.has(id))
    if (!counted.length) return {}
    const rows = await this.db.channelMessage.groupBy({
      by: ['channelId'],
      where: { channelId: { in: counted }, authorId: { not: userId } },
      _count: { _all: true },
      _max: { createdAt: true },
    })
    const totals: Record<string, number> = {}
    // Son okuma tarihi olan kanallarda yalnız sonrası sayılır; gruplama tek sorguda
    // tarih filtresi alamadığı için bu kanallar ayrıca sayılır.
    await Promise.all(rows.map(async row => {
      const since = lastRead.get(row.channelId)
      const projectId = channelProject.get(row.channelId)!
      if (!since) { totals[projectId] = (totals[projectId] || 0) + row._count._all; return }
      if (row._max.createdAt && row._max.createdAt <= since) return
      const count = await this.db.channelMessage.count({ where: { channelId: row.channelId, authorId: { not: userId }, createdAt: { gt: since } } })
      if (count) totals[projectId] = (totals[projectId] || 0) + count
    }))
    return totals
  }

  @Post()
  async create(@Param('organizationId') organizationId: string, @Req() req: any, @Dto(CreateProjectDto) dto: CreateProjectDto) {
    await this.membership(organizationId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'project.create')
    const slug = dto.slug.trim().toLowerCase()
    if (await this.db.project.findUnique({ where: { organizationId_slug: { organizationId, slug } } })) throw new ConflictException({ code: 'PROJECT_SLUG_TAKEN', message: 'Bu proje kodu kuruluşta zaten kullanılıyor.', field: 'slug' })
    const project = await this.db.project.create({ data: { organizationId, ownerId: req.user.sub, name: dto.name.trim(), slug, description: dto.description?.trim() || null, engine: dto.engine || null, members: { create: { userId: req.user.sub, role: 'OWNER' } }, channelCategories: { create: [
      { name: 'GENEL', position: 0, channels: { create: [{ name: 'başlangıç', type: 'ANNOUNCEMENT', position: 0 }, { name: 'genel', type: 'TEXT', position: 1 }] } },
      { name: 'ÇALIŞMA', position: 1, channels: { create: [{ name: 'geliştirme', type: 'TEXT', position: 0 }, { name: 'geri-bildirim', type: 'FORUM', position: 1 }] } },
      { name: 'TOPLANTI', position: 2, channels: { create: [{ name: 'ekip-odası', type: 'VOICE', position: 0 }] } },
    ] } }, select: { id: true, name: true, slug: true, description: true, engine: true, ownerId: true, defaultChannelId: true, coverUrl: true, iconUrl: true, libraryEnabled: true, createdAt: true, updatedAt: true } })
    this.stream.emitToUsers([req.user.sub], 'project:created', { project })
    return project
  }

  // Proje ayarları: başlangıç kanalı. Kanalları yönetebilen herkes değiştirebilir.
  @Patch(':projectId')
  async updateProject(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Body() body: UpdateProjectDto) {
    await this.membership(organizationId, req.user.sub)
    await requireProjectAccess(this.db, organizationId, req.user.sub, projectId)
    await requirePermission(this.db, organizationId, req.user.sub, 'channel.manage')
    const data: { defaultChannelId?: string | null; coverUrl?: string | null; iconUrl?: string | null; description?: string | null; libraryEnabled?: boolean } = {}
    if (body?.libraryEnabled !== undefined) { await requirePermission(this.db, organizationId, req.user.sub, 'release.publish'); data.libraryEnabled = Boolean(body.libraryEnabled) }
    const uploadUrl = (value: unknown) => value === null || value === '' ? null : typeof value === 'string' && /^\/uploads\/[a-f0-9-]{36}(\.[a-zA-Z0-9]{1,10})?$/.test(value) ? value : undefined
    if (body?.description !== undefined) data.description = String(body.description).trim() || null
    if (body?.coverUrl !== undefined) { const url = uploadUrl(body.coverUrl); if (url === undefined) throw new NotFoundException({ code: 'UPLOAD_NOT_FOUND', message: 'Görsel bulunamadı.' }); data.coverUrl = url }
    if (body?.iconUrl !== undefined) { const url = uploadUrl(body.iconUrl); if (url === undefined) throw new NotFoundException({ code: 'UPLOAD_NOT_FOUND', message: 'Görsel bulunamadı.' }); data.iconUrl = url }
    if (body?.defaultChannelId !== undefined) {
      if (body.defaultChannelId === null || body.defaultChannelId === '') data.defaultChannelId = null
      else {
        const channel = await this.db.projectChannel.findFirst({ where: { id: String(body.defaultChannelId), deletedAt: null, category: { projectId, deletedAt: null } }, select: { id: true } })
        if (!channel) throw new NotFoundException({ code: 'CHANNEL_NOT_FOUND', message: 'Kanal bulunamadı.' })
        data.defaultChannelId = channel.id
      }
    }
    const project = await this.db.project.update({ where: { id: projectId }, data, select: { id: true, name: true, slug: true, description: true, engine: true, ownerId: true, defaultChannelId: true, coverUrl: true, iconUrl: true, libraryEnabled: true, createdAt: true, updatedAt: true } })
    void this.stream.emitProject(organizationId, projectId, 'project:updated', { project })
    return project
  }

  @Delete(':projectId')
  async deleteProject(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any) {
    const member = await this.membership(organizationId, req.user.sub)
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId, deletedAt: null } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    if (project.ownerId !== req.user.sub && !['OWNER', 'ADMIN'].includes(member.role)) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    const deletedAt = new Date(), purgeAt = new Date(deletedAt.getTime() + 30 * 24 * 60 * 60 * 1000)
    await this.stream.emitProject(organizationId, projectId, 'project:deleted', { projectId })
    await this.db.project.update({ where: { id: projectId }, data: { deletedAt, purgeAt } })
    return { deletedAt, purgeAt }
  }

  @Get(':projectId/channels')
  async channels(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    await requireProjectAccess(this.db, organizationId, req.user.sub, projectId)
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    if (!(await this.db.channelCategory.count({ where: { projectId } }))) await this.db.$transaction([
      this.db.channelCategory.create({ data: { projectId, name: 'GENEL', position: 0, channels: { create: [{ name: 'başlangıç', type: 'ANNOUNCEMENT', position: 0 }, { name: 'genel', type: 'TEXT', position: 1 }] } } }),
      this.db.channelCategory.create({ data: { projectId, name: 'ÇALIŞMA', position: 1, channels: { create: [{ name: 'geliştirme', type: 'TEXT', position: 0 }, { name: 'geri-bildirim', type: 'FORUM', position: 1 }] } } }),
      this.db.channelCategory.create({ data: { projectId, name: 'TOPLANTI', position: 2, channels: { create: [{ name: 'ekip-odası', type: 'VOICE', position: 0 }] } } }),
    ])
    const categories = await this.db.channelCategory.findMany({ where: { projectId, deletedAt: null }, orderBy: { position: 'asc' }, select: { id: true, name: true, position: true, channels: { where: { deletedAt: null }, orderBy: { position: 'asc' }, select: { id: true, name: true, topic: true, type: true, position: true, reads:{where:{userId:req.user.sub},select:{lastReadAt:true}} } } } })
    // "Kanalı görüntüle" yetkisi olmayan kanallar listeye hiç girmez.
    const visible = await visibleChannelIds(this.db, organizationId, req.user.sub, categories.flatMap(category => category.channels.map(channel => channel.id)))
    return Promise.all(categories.map(async category => ({ ...category, channels: await Promise.all(category.channels.filter(channel => visible.has(channel.id)).map(async channel => {
      const lastReadAt=channel.reads[0]?.lastReadAt
      const unread=await this.db.channelMessage.count({where:{channelId:channel.id,authorId:{not:req.user.sub},...(lastReadAt?{createdAt:{gt:lastReadAt}}:{})}})
      const {reads,...view}=channel
      return {...view,unread}
    })) })))
  }

  @Post(':projectId/channels/:channelId/read')
  async readChannel(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('channelId') channelId:string,@Req() req:any){
    await this.membership(organizationId,req.user.sub)
    await requireChannelPermission(this.db,organizationId,req.user.sub,channelId,'channel.view')
    await this.activeChannel(organizationId,projectId,channelId,req.user.sub)
    await this.db.channelRead.upsert({where:{channelId_userId:{channelId,userId:req.user.sub}},create:{channelId,userId:req.user.sub,lastReadAt:new Date()},update:{lastReadAt:new Date()}})
    return {ok:true}
  }

  @Post(':projectId/channels')
  async createChannel(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Dto(CreateChannelDto) dto: CreateChannelDto) {
    await this.membership(organizationId, req.user.sub)
    await requireProjectAccess(this.db, organizationId, req.user.sub, projectId)
    await requirePermission(this.db, organizationId, req.user.sub, 'channel.manage')
    const category = await this.db.channelCategory.findFirst({ where: { id: dto.categoryId, projectId, project: { organizationId } } })
    if (!category) throw new NotFoundException({ code: 'CHANNEL_CATEGORY_NOT_FOUND', message: 'Kanal kategorisi bulunamadı.' })
    const name = dto.name.trim()
    if (!name) throw new ConflictException({ code: 'CHANNEL_NAME_INVALID', message: 'Geçerli bir kanal adı gir.', field: 'name' })
    if (await this.db.projectChannel.findFirst({ where: { categoryId: category.id, name: { equals: name, mode: 'insensitive' }, deletedAt: null } })) throw new ConflictException({ code: 'CHANNEL_NAME_TAKEN', message: 'Bu kategoride aynı isimde bir kanal var.', field: 'name' })
    const last = await this.db.projectChannel.aggregate({ where: { categoryId: category.id, deletedAt: null }, _max: { position: true } })
    const channel = await this.db.projectChannel.create({ data: { categoryId: category.id, name, type: dto.type, position: (last._max.position ?? -1) + 1 }, select: { id: true, name: true, topic: true, type: true, position: true } })
    void this.stream.emitChannel(organizationId, channel.id, 'channel:created', { projectId, categoryId: category.id, channel })
    return channel
  }

  private async activeChannel(organizationId: string, projectId: string, channelId: string, userId: string) {
    await requireProjectAccess(this.db, organizationId, userId, projectId)
    const channel = await this.db.projectChannel.findFirst({ where: { id: channelId, deletedAt: null, category: { projectId, project: { organizationId } } } })
    if (!channel) throw new NotFoundException({ code: 'CHANNEL_NOT_FOUND', message: 'Kanal bulunamadı.' })
    return channel
  }

  // Kanal ayarları: ad ve konu. Konu, kanal başlığında açıklamanın yerini alır.
  @Patch(':projectId/channels/:channelId')
  async updateChannel(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Req() req: any, @Dto(UpdateChannelDto) dto: UpdateChannelDto) {
    await this.membership(organizationId, req.user.sub)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.manage')
    const current = await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    const data: { name?: string; topic?: string | null } = {}
    if (dto.name !== undefined) {
      const name = dto.name.trim()
      if (!name) throw new ConflictException({ code: 'CHANNEL_NAME_INVALID', message: 'Geçerli bir kanal adı gir.', field: 'name' })
      if (name.toLocaleLowerCase('tr-TR') !== current.name.toLocaleLowerCase('tr-TR') && await this.db.projectChannel.findFirst({ where: { categoryId: current.categoryId, name: { equals: name, mode: 'insensitive' }, deletedAt: null } })) throw new ConflictException({ code: 'CHANNEL_NAME_TAKEN', message: 'Bu kategoride aynı isimde bir kanal var.', field: 'name' })
      data.name = name
    }
    if (dto.topic !== undefined) data.topic = dto.topic.trim() || null
    const channel = await this.db.projectChannel.update({ where: { id: channelId }, data, select: { id: true, name: true, topic: true, type: true, position: true } })
    void this.stream.emitChannel(organizationId, channelId, 'channel:updated', { projectId, categoryId: current.categoryId, channel })
    return channel
  }

  @Delete(':projectId/channels/:channelId')
  async deleteChannel(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.manage')
    await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    const deletedAt = new Date(), purgeAt = new Date(deletedAt.getTime() + 30 * 24 * 60 * 60 * 1000)
    await this.db.projectChannel.update({ where: { id: channelId }, data: { deletedAt, purgeAt } })
    this.stream.emit(organizationId, 'channel:deleted', { projectId, channelId })
    return { deletedAt, purgeAt }
  }

  @Delete(':projectId/channel-categories/:categoryId')
  async deleteCategory(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('categoryId') categoryId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    await requireProjectAccess(this.db, organizationId, req.user.sub, projectId)
    await requirePermission(this.db, organizationId, req.user.sub, 'channel.manage')
    const category = await this.db.channelCategory.findFirst({ where: { id: categoryId, projectId, deletedAt: null, project: { organizationId } }, include: { channels: { select: { deletedAt: true, purgeAt: true } } } })
    if (!category) throw new NotFoundException({ code: 'CHANNEL_CATEGORY_NOT_FOUND', message: 'Kanal kategorisi bulunamadı.' })
    if (category.channels.some(channel => !channel.deletedAt)) throw new ConflictException({ code: 'CHANNEL_CATEGORY_NOT_EMPTY', message: 'Kategoriyi silmek için önce içindeki kanalları silmelisin.' })
    const deletedAt = new Date()
    const childPurgeTimes = category.channels.flatMap(channel => channel.purgeAt ? [channel.purgeAt.getTime()] : [])
    const purgeAt = new Date(Math.max(deletedAt.getTime() + 30 * 24 * 60 * 60 * 1000, ...childPurgeTimes))
    await this.db.channelCategory.update({ where: { id: categoryId }, data: { deletedAt, purgeAt } })
    void this.stream.emitProject(organizationId, projectId, 'category:deleted', { projectId, categoryId })
    return { deletedAt, purgeAt }
  }

  @Post(':projectId/channel-categories')
  async createCategory(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Dto(CreateCategoryDto) dto: CreateCategoryDto) {
    await this.membership(organizationId, req.user.sub)
    await requireProjectAccess(this.db, organizationId, req.user.sub, projectId)
    await requirePermission(this.db, organizationId, req.user.sub, 'channel.manage')
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    const name = dto.name.trim()
    if (!name) throw new ConflictException({ code: 'CATEGORY_NAME_INVALID', message: 'Geçerli bir kategori adı gir.', field: 'name' })
    if (await this.db.channelCategory.findFirst({ where: { projectId, name: { equals: name, mode: 'insensitive' }, deletedAt: null } })) throw new ConflictException({ code: 'CATEGORY_NAME_TAKEN', message: 'Aynı isimde bir kategori zaten var.', field: 'name' })
    const last = await this.db.channelCategory.aggregate({ where: { projectId, deletedAt: null }, _max: { position: true } })
    const category = await this.db.channelCategory.create({ data: { projectId, name, position: (last._max.position ?? -1) + 1 }, select: { id: true, name: true, position: true, channels: { select: { id: true, name: true, type: true, position: true } } } })
    void this.stream.emitProject(organizationId, projectId, 'category:created', { projectId, category })
    return category
  }

  // Sayfalama: ?before=<ISO createdAt>&limit=<1..100>; parametresiz çağrı en yeni 100 mesajı artan sırada döner.
  @Get(':projectId/channels/:channelId/messages')
  async messages(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Query('before') before: string | undefined, @Query('limit') limit: string | undefined, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.view')
    await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    const page = pageArgs(before, limit, { take: 100, max: 100 })
    const rows = (await this.db.channelMessage.findMany({ where: { channelId, parentId: null, ...page.where }, orderBy: { createdAt: 'desc' }, take: page.take, select: { id: true, reactions: { orderBy: { createdAt: 'asc' }, select: { emoji: true, userId: true } }, content: true, createdAt: true, editedAt: true, forwardedFromId: true, quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } }, _count: { select: { replies: true } }, replies: { orderBy: { createdAt: 'desc' }, take: 1, select: { createdAt: true, author: { select: { displayName: true } } } }, author: { select: { id: true, username: true, displayName: true } } } })).reverse()
    return rows.map(row => ({ ...row, reactions: groupReactions(row.reactions, req.user.sub) }))
  }

  // Kanal içi mesaj arama
  @Get(':projectId/channels/:channelId/search')
  async search(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Query('q') q: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.view')
    await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    const term = String(q || '').trim()
    if (term.length < 2) return []
    return this.db.channelMessage.findMany({
      where: { channelId, content: { contains: term, mode: 'insensitive' } },
      orderBy: { createdAt: 'desc' },
      take: 50,
      select: { id: true, content: true, createdAt: true, parentId: true, author: { select: { id: true, username: true, displayName: true } } },
    })
  }

  @Post(':projectId/channels/:channelId/messages')
  async createMessage(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Req() req: any, @Dto(CreateMessageDto) dto: CreateMessageDto) {
    await this.membership(organizationId, req.user.sub)
    await this.assertCanPost(organizationId, req.user.sub, channelId, dto.content)
    const channel = await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    if (dto.quotedMessageId && !(await this.db.channelMessage.findFirst({ where: { id: dto.quotedMessageId, channelId } }))) throw new NotFoundException({ code: 'QUOTED_MESSAGE_NOT_FOUND', message: 'Alıntılanan mesaj bulunamadı.' })
    const message = await this.db.channelMessage.create({ data: { channelId, authorId: req.user.sub, content: dto.content.trim(), quotedMessageId: dto.quotedMessageId || null }, select: { id: true, content: true, createdAt: true, editedAt: true, forwardedFromId: true, quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } }, author: { select: { id: true, username: true, displayName: true } } } })
    void this.stream.emitChannel(organizationId, channelId, 'message:created', { projectId, channelId, channelName: channel.name, parentId: null, message: { ...message, _count: { replies: 0 } } })
    await this.notifyMentions(organizationId, req.user.sub, dto.content, { projectId, channelId, channelName: channel.name, messageId: message.id, parentId: null })
    return message
  }


  // Kanala yazma: görüntüleme + mesaj gönderme; içerikte dosya varsa dosya ekleme,
  // @everyone/@here varsa herkesi etiketleme yetkisi de aranır.
  private async assertCanPost(organizationId: string, userId: string, channelId: string, content: string) {
    const needed: Array<'channel.view' | 'message.send' | 'message.attach' | 'message.mention_all'> = ['channel.view', 'message.send']
    if (/\[\[p2-file\|/.test(content)) needed.push('message.attach')
    if (/@(everyone|here)\b/.test(content)) needed.push('message.mention_all')
    await requireChannelPermission(this.db, organizationId, userId, channelId, ...needed)
  }
  // @kullanıcı adı geçen kişilere etkinlik kaydı düşer.
  private async notifyMentions(organizationId: string, actorId: string, content: string, context: { projectId: string; channelId: string; channelName: string; messageId: string; parentId: string | null }) {
    const mentioned = await mentionedUserIds(this.db, organizationId, content)
    const targets = await channelAudience(this.db, organizationId, context.channelId, mentioned)
    if (!targets.length) return
    await recordActivity(this.db, this.stream, targets.map(userId => ({
      organizationId, userId, actorId, kind: 'MENTION' as const, preview: content, ...context,
    })))
  }

  @Patch(':projectId/channels/:channelId/messages/:messageId')
  async editMessage(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Param('messageId') messageId: string, @Req() req: any, @Dto(EditMessageDto) dto: EditMessageDto) {
    await this.membership(organizationId, req.user.sub);await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    await this.assertCanPost(organizationId, req.user.sub, channelId, dto.content)
    const message = await this.db.channelMessage.findFirst({ where: { id: messageId, channelId, authorId: req.user.sub } })
    if (!message) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    const content=dto.content.trim();if(content===message.content)return { ...message }
    const updated=await this.db.$transaction(async tx=>{await tx.messageEditHistory.create({data:{messageId,content:message.content}});return tx.channelMessage.update({where:{id:messageId},data:{content,editedAt:new Date()},select:{id:true,content:true,createdAt:true,editedAt:true,forwardedFromId:true,quotedMessage:{select:{id:true,content:true,author:{select:{displayName:true}}}},author:{select:{id:true,username:true,displayName:true}}}})})
    void this.stream.emitChannel(organizationId, channelId, 'message:updated',{projectId,channelId,parentId:message.parentId,message:updated})
    return updated
  }

  @Get(':projectId/channels/:channelId/messages/:messageId/history')
  async messageHistory(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('channelId') channelId:string,@Param('messageId') messageId:string,@Req() req:any){await this.membership(organizationId,req.user.sub);await this.activeChannel(organizationId,projectId,channelId,req.user.sub);await requireChannelPermission(this.db,organizationId,req.user.sub,channelId,'channel.view');if(!(await this.db.channelMessage.findFirst({where:{id:messageId,channelId}})))throw new NotFoundException({code:'MESSAGE_NOT_FOUND',message:'Mesaj bulunamadı.'});return this.db.messageEditHistory.findMany({where:{messageId},orderBy:{createdAt:'desc'},select:{id:true,content:true,createdAt:true}})}

  @Delete(':projectId/channels/:channelId/messages/:messageId')
  async deleteMessage(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('channelId') channelId:string,@Param('messageId') messageId:string,@Req() req:any){await this.membership(organizationId,req.user.sub);await this.activeChannel(organizationId,projectId,channelId,req.user.sub);const message=await this.db.channelMessage.findFirst({where:{id:messageId,channelId}});if(!message)throw new NotFoundException({code:'MESSAGE_NOT_FOUND',message:'Mesaj bulunamadı.'});const {permissions}=await channelPermissions(this.db,organizationId,req.user.sub,channelId);if(!permissions.has('channel.view'))await requireChannelPermission(this.db,organizationId,req.user.sub,channelId,'channel.view');const moderator=message.authorId!==req.user.sub;const unlimited=permissions.has('message.delete_unlimited');if(moderator&&!unlimited&&!permissions.has('message.delete_any'))await requireChannelPermission(this.db,organizationId,req.user.sub,channelId,'message.delete_any');
    // Normal kullanıcı kendi mesajını 2 saat içinde silebilir; süresiz silme yetkisi bu sınırı kaldırır.
    if(!moderator&&!unlimited&&Date.now()-message.createdAt.getTime()>2*60*60*1000)throw new ConflictException({code:'MESSAGE_DELETE_WINDOW_EXPIRED',message:'Mesajlar yalnızca ilk 2 saat içinde silinebilir.'});await this.db.channelMessage.delete({where:{id:messageId}});const parentReplies=message.parentId?await this.db.channelMessage.count({where:{parentId:message.parentId}}):0;void this.stream.emitChannel(organizationId, channelId, 'message:deleted',{projectId,channelId,messageId,parentId:message.parentId,parentReplies});return {deleted:true}}

  @Post(':projectId/channels/:channelId/messages/:messageId/forward')
  async forwardMessage(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('channelId') channelId:string,@Param('messageId') messageId:string,@Req() req:any,@Dto(ForwardMessageDto) dto: ForwardMessageDto){await this.membership(organizationId,req.user.sub);await this.activeChannel(organizationId,projectId,channelId,req.user.sub);const source=await this.db.channelMessage.findFirst({where:{id:messageId,channelId}});if(!source)throw new NotFoundException({code:'MESSAGE_NOT_FOUND',message:'Mesaj bulunamadı.'});await this.activeChannel(organizationId,projectId,dto.targetChannelId,req.user.sub);await requireChannelPermission(this.db,organizationId,req.user.sub,channelId,'channel.view','message.forward');await requireChannelPermission(this.db,organizationId,req.user.sub,dto.targetChannelId,'channel.view','message.send');const message=await this.db.channelMessage.create({data:{channelId:dto.targetChannelId,authorId:req.user.sub,content:source.content,forwardedFromId:source.id},select:{id:true,content:true,createdAt:true,editedAt:true,forwardedFromId:true,author:{select:{id:true,username:true,displayName:true}}}});void this.stream.emitChannel(organizationId, channelId, 'message:created',{projectId,channelId:dto.targetChannelId,parentId:null,message:{...message,_count:{replies:0}}});return message}

  @Post(':projectId/channels/:channelId/messages/:messageId/reactions')
  async toggleReaction(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('channelId') channelId:string,@Param('messageId') messageId:string,@Req() req:any,@Dto(ReactMessageDto) dto: ReactMessageDto){await this.membership(organizationId,req.user.sub);await this.activeChannel(organizationId,projectId,channelId,req.user.sub);await requireChannelPermission(this.db,organizationId,req.user.sub,channelId,'channel.view','message.react');const target=await this.db.channelMessage.findFirst({where:{id:messageId,channelId},select:{id:true,authorId:true,content:true,parentId:true}});if(!target)throw new NotFoundException({code:'MESSAGE_NOT_FOUND',message:'Mesaj bulunamadı.'});const emoji=dto.emoji.trim();const userId=req.user.sub as string;const existing=await this.db.messageReaction.findUnique({where:{messageId_userId_emoji:{messageId,userId,emoji}}});if(existing)await this.db.messageReaction.delete({where:{id:existing.id}});else{await this.db.messageReaction.create({data:{messageId,userId,emoji}});const channelRow=await this.db.projectChannel.findUnique({where:{id:channelId},select:{name:true}});await recordActivity(this.db,this.stream,[{organizationId,userId:target.authorId,actorId:userId,kind:'REACTION',preview:target.content,emoji,projectId,channelId,channelName:channelRow?.name??null,messageId:target.id,parentId:target.parentId}])}const rows=await this.db.messageReaction.findMany({where:{messageId},orderBy:{createdAt:'asc'},select:{emoji:true,userId:true}});void this.stream.emitChannel(organizationId, channelId, 'message:reactions',{projectId,channelId,messageId,rows});return groupReactions(rows,userId)}

  @Get(':projectId/channels/:channelId/messages/:messageId/replies')
  async messageReplies(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Param('messageId') messageId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.view')
    const message = await this.db.channelMessage.findFirst({ where: { id: messageId, channelId, parentId: null } })
    if (!message) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    const rows = await this.db.channelMessage.findMany({ where: { channelId, parentId: messageId }, orderBy: { createdAt: 'asc' }, select: { id: true, reactions: { orderBy: { createdAt: 'asc' }, select: { emoji: true, userId: true } }, content: true, createdAt: true, editedAt: true, quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } }, author: { select: { id: true, username: true, displayName: true } } } })
    return rows.map(row => ({ ...row, reactions: groupReactions(row.reactions, req.user.sub) }))
  }

  @Post(':projectId/channels/:channelId/messages/:messageId/replies')
  async createMessageReply(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Param('messageId') messageId: string, @Req() req: any, @Dto(CreateMessageDto) dto: CreateMessageDto) {
    await this.membership(organizationId, req.user.sub)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.view', 'thread.reply')
    const channel = await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    const message = await this.db.channelMessage.findFirst({ where: { id: messageId, channelId, parentId: null } })
    if (!message) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    if (dto.quotedMessageId && !(await this.db.channelMessage.findFirst({ where: { id: dto.quotedMessageId, channelId } }))) throw new NotFoundException({ code: 'QUOTED_MESSAGE_NOT_FOUND', message: 'Alıntılanan mesaj bulunamadı.' })
    const reply = await this.db.channelMessage.create({ data: { channelId, parentId: messageId, authorId: req.user.sub, content: dto.content.trim(), quotedMessageId: dto.quotedMessageId || null }, select: { id: true, content: true, createdAt: true, editedAt: true, quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } }, author: { select: { id: true, username: true, displayName: true } } } })
    const parentReplies = await this.db.channelMessage.count({ where: { parentId: messageId } })
    void this.stream.emitChannel(organizationId, channelId, 'message:created', { projectId, channelId, channelName: channel.name, parentId: messageId, parentReplies, message: reply })
    await recordActivity(this.db, this.stream, [{
      organizationId, userId: message.authorId, actorId: req.user.sub, kind: 'REPLY', preview: dto.content,
      projectId, channelId, channelName: channel.name, messageId: reply.id, parentId: messageId,
    }])
    await this.notifyMentions(organizationId, req.user.sub, dto.content, { projectId, channelId, channelName: channel.name, messageId: reply.id, parentId: messageId })
    return { ...reply, parentReplies }
  }

  @Get(':projectId/channels/:channelId/topics/:topicId/replies')
  async topicReplies(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Param('topicId') topicId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    const channel = await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.view')
    if (channel.type !== 'FORUM') throw new ConflictException({ code: 'CHANNEL_NOT_FORUM', message: 'Bu kanal forum değil.' })
    const topic = await this.db.channelMessage.findFirst({ where: { id: topicId, channelId, parentId: null } })
    if (!topic) throw new NotFoundException({ code: 'TOPIC_NOT_FOUND', message: 'Konu bulunamadı.' })
    const rows = await this.db.channelMessage.findMany({ where: { channelId, parentId: topicId }, orderBy: { createdAt: 'asc' }, select: { id: true, reactions: { orderBy: { createdAt: 'asc' }, select: { emoji: true, userId: true } }, content: true, createdAt: true, editedAt: true, quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } }, author: { select: { id: true, username: true, displayName: true } } } })
    return rows.map(row => ({ ...row, reactions: groupReactions(row.reactions, req.user.sub) }))
  }

  @Post(':projectId/channels/:channelId/topics/:topicId/replies')
  async createTopicReply(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Param('topicId') topicId: string, @Req() req: any, @Dto(CreateMessageDto) dto: CreateMessageDto) {
    await this.membership(organizationId, req.user.sub)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.view', 'thread.reply')
    const channel = await this.activeChannel(organizationId, projectId, channelId, req.user.sub)
    if (channel.type !== 'FORUM') throw new ConflictException({ code: 'CHANNEL_NOT_FORUM', message: 'Bu kanal forum değil.' })
    const topic = await this.db.channelMessage.findFirst({ where: { id: topicId, channelId, parentId: null } })
    if (!topic) throw new NotFoundException({ code: 'TOPIC_NOT_FOUND', message: 'Konu bulunamadı.' })
    if (dto.quotedMessageId && !(await this.db.channelMessage.findFirst({ where: { id: dto.quotedMessageId, channelId } }))) throw new NotFoundException({ code: 'QUOTED_MESSAGE_NOT_FOUND', message: 'Alıntılanan mesaj bulunamadı.' })
    const reply = await this.db.channelMessage.create({ data: { channelId, parentId: topicId, authorId: req.user.sub, content: dto.content.trim(), quotedMessageId: dto.quotedMessageId || null }, select: { id: true, content: true, createdAt: true, editedAt: true, quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } }, author: { select: { id: true, username: true, displayName: true } } } })
    const parentReplies = await this.db.channelMessage.count({ where: { parentId: topicId } })
    void this.stream.emitChannel(organizationId, channelId, 'message:created', { projectId, channelId, channelName: channel.name, parentId: topicId, parentReplies, message: reply })
    return { ...reply, parentReplies }
  }
}
