import { BadRequestException, Body, ConflictException, Controller, Delete, ForbiddenException, Get, Inject, NotFoundException, Param, Patch, Post, Req, UseGuards } from '@nestjs/common'
import { IsIn, IsInt as IsWholeNumber, IsOptional, IsString, Length, Max, Min } from 'class-validator'
import { randomBytes } from 'node:crypto'
import { readdir, stat } from 'node:fs/promises'
import { join } from 'node:path'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { applyChannelOverrides, cleanPermissions, effectivePermissions, PERMISSIONS, requirePermission, type PermissionId } from './permissions'


export class OrganizationDto { @IsOptional() @IsString() @Length(2, 60) name?: string }
export class InviteDto {
  @IsOptional() @IsWholeNumber() @Min(1) @Max(1000) maxUses?: number
  @IsOptional() @IsWholeNumber() @Min(1) @Max(365) expiresInDays?: number
}

const uploadRoot = () => process.env.P2_UPLOAD_DIR || join(process.cwd(), '.p2-data', 'uploads')

// Yüklenen dosyalar diskte tutuluyor; toplam boyut klasörden okunur.
async function storageUsed() {
  try {
    const names = await readdir(uploadRoot())
    let total = 0, files = 0
    for (const name of names) {
      try { const info = await stat(join(uploadRoot(), name)); if (info.isFile()) { total += info.size; files++ } } catch {}
    }
    return { bytes: total, files }
  } catch { return { bytes: 0, files: 0 } }
}

// Kuruluş yönetim paneli. Her uç kendi yetkisini ister (admin.panel, member.kick,
// member.invite, member.roles, organization.manage); sahiplik devri sahibe özeldir.
@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/admin')
export class AdminController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  // Panelin her ucu bir yetki ister. Yetkiler role, kullanıcı gruplarına ve kişiye
  // özel eklemelere göre hesaplanır; artık salt "yönetici mi?" kontrolü yok.
  private async staff(organizationId: string, userId: string, permission: PermissionId = 'admin.panel') {
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
    await requirePermission(this.db, organizationId, userId, permission)
    return member
  }

  @Get('overview')
  async overview(@Param('organizationId') organizationId: string, @Req() req: any) {
    await this.staff(organizationId, req.user.sub)
    const organization = await this.db.organization.findUniqueOrThrow({ where: { id: organizationId }, select: { id: true, name: true, slug: true, createdAt: true } })
    const [members, projects, channels, deletedChannels, groups, channelMessages, directMessages, groupMessages, invites, twoFactor, roleCount, sessions] = await Promise.all([
      this.db.organizationMember.findMany({ where: { organizationId }, select: { role: true, joinedAt: true, user: { select: { status: true, twoFactorEnabled: true } } } }),
      this.db.project.count({ where: { organizationId } }),
      this.db.projectChannel.count({ where: { deletedAt: null, category: { project: { organizationId } } } }),
      this.db.projectChannel.count({ where: { deletedAt: { not: null }, category: { project: { organizationId } } } }),
      this.db.chatGroup.count({ where: { organizationId } }),
      this.db.channelMessage.count({ where: { channel: { category: { project: { organizationId } } } } }),
      this.db.directMessage.count({ where: { conversation: { organizationId } } }),
      this.db.chatGroupMessage.count({ where: { group: { organizationId } } }),
      this.db.organizationInvite.count({ where: { organizationId } }),
      this.db.organizationMember.count({ where: { organizationId, user: { twoFactorEnabled: true } } }),
      this.db.userGroup.count({ where: { organizationId } }),
      this.db.session.count({ where: { expiresAt: { gt: new Date() }, user: { organizationMemberships: { some: { organizationId } } } } }),
    ])
    const week = Date.now() - 7 * 86400000
    return {
      organization,
      members: {
        total: members.length,
        owners: members.filter(item => item.role === 'OWNER').length,
        roles: roleCount,
        online: members.filter(item => item.user.status !== 'OFFLINE').length,
        joinedThisWeek: members.filter(item => item.joinedAt.getTime() > week).length,
        twoFactor,
      },
      content: { projects, channels, deletedChannels, groups, channelMessages, directMessages, groupMessages, messages: channelMessages + directMessages + groupMessages },
      invites,
      sessions,
      storage: await storageUsed(),
    }
  }

  @Get('members')
  async members(@Param('organizationId') organizationId: string, @Req() req: any) {
    await this.staff(organizationId, req.user.sub)
    const rows = await this.db.organizationMember.findMany({
      where: { organizationId }, orderBy: { joinedAt: 'asc' },
      select: { role: true, joinedAt: true, user: { select: { id: true, username: true, displayName: true, email: true, title: true, avatarUrl: true, status: true, twoFactorEnabled: true, createdAt: true } } },
    })
    const counts = await this.db.channelMessage.groupBy({ by: ['authorId'], where: { channel: { category: { project: { organizationId } } } }, _count: { _all: true } })
    const byAuthor = new Map(counts.map(row => [row.authorId, row._count._all]))
    // Kişinin rolleri: doğrudan atananlar + gruplarından gelenler. Grupları da ayrıca gösteriyoruz.
    const roleRows = await this.db.role.findMany({
      where: { organizationId, everyone: false },
      orderBy: [{ founder: 'desc' }, { handle: 'asc' }],
      select: { id: true, handle: true, name: true, color: true, founder: true, members: { select: { userId: true } }, groups: { select: { group: { select: { members: { select: { userId: true } } } } } } },
    })
    const groupRows = await this.db.userGroupMember.findMany({ where: { group: { organizationId } }, select: { userId: true, group: { select: { id: true, handle: true, name: true, color: true } } } })
    const rolesOf = new Map<string, { id: string; handle: string; name: string; color: string | null; founder: boolean; viaGroup: boolean }[]>()
    for (const role of roleRows) {
      const direct = new Set(role.members.map(item => item.userId))
      const viaGroups = new Set(role.groups.flatMap(item => item.group.members.map(member => member.userId)))
      for (const userId of new Set([...direct, ...viaGroups])) {
        rolesOf.set(userId, [...(rolesOf.get(userId) || []), { id: role.id, handle: role.handle, name: role.name, color: role.color, founder: role.founder, viaGroup: !direct.has(userId) }])
      }
    }
    const groupsOf = new Map<string, { id: string; handle: string; name: string; color: string | null }[]>()
    for (const row of groupRows) groupsOf.set(row.userId, [...(groupsOf.get(row.userId) || []), row.group])
    return rows.map(row => ({ ...row.user, owner: row.role === 'OWNER', roles: rolesOf.get(row.user.id) || [], groups: groupsOf.get(row.user.id) || [], joinedAt: row.joinedAt, channelMessages: byAuthor.get(row.user.id) || 0 }))
  }

  // Sahiplik devri. Sahiplik bir rol değil: kuruluşu yöneten tek kişiyi işaret eder
  // ve yalnızca mevcut sahip devredebilir.
  @Post('members/:userId/transfer-owner')
  async transferOwner(@Param('organizationId') organizationId: string, @Param('userId') userId: string, @Req() req: any) {
    const me = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId: req.user.sub } } })
    if (!me || me.role !== 'OWNER') throw new ForbiddenException({ code: 'OWNER_ONLY', message: 'Sahipliği yalnızca kuruluş sahibi devredebilir.' })
    if (userId === req.user.sub) throw new ConflictException({ code: 'SELF_OWNER', message: 'Sahiplik zaten sende.' })
    const target = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!target) throw new NotFoundException({ code: 'MEMBER_NOT_FOUND', message: 'Üye bulunamadı.' })
    const founder = await this.db.role.findFirst({ where: { organizationId, founder: true }, select: { id: true } })
    await this.db.$transaction([
      this.db.organizationMember.update({ where: { organizationId_userId: { organizationId, userId } }, data: { role: 'OWNER' } }),
      this.db.organizationMember.update({ where: { organizationId_userId: { organizationId, userId: req.user.sub } }, data: { role: 'MEMBER' } }),
      // Kurucu rolü kuruluş sahipliğini izler.
      ...(founder ? [
        this.db.roleMember.deleteMany({ where: { roleId: founder.id } }),
        this.db.roleMember.create({ data: { roleId: founder.id, userId } }),
      ] : []),
    ])
    for (const id of [userId, req.user.sub]) {
      const after = await effectivePermissions(this.db, organizationId, id)
      this.stream.emitToUsers([id], 'permissions:changed', { organizationId, permissions: after.permissions })
    }
    return { owner: userId }
  }

  // Üyeyi çıkarma: sahip çıkarılamaz, kimse kendini çıkaramaz. Oturumları da kapanır.
  @Delete('members/:userId')
  async removeMember(@Param('organizationId') organizationId: string, @Param('userId') userId: string, @Req() req: any) {
    const me = await this.staff(organizationId, req.user.sub, 'member.kick')
    if (userId === me.userId) throw new ConflictException({ code: 'SELF_REMOVE', message: 'Kendini kuruluştan çıkaramazsın.' })
    const target = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!target) throw new NotFoundException({ code: 'MEMBER_NOT_FOUND', message: 'Üye bulunamadı.' })
    if (target.role === 'OWNER') throw new ConflictException({ code: 'OWNER_PROTECTED', message: 'Kuruluş sahibi çıkarılamaz.' })
    if (me.role === 'ADMIN' && target.role === 'ADMIN') throw new ForbiddenException({ code: 'ADMIN_PROTECTED', message: 'Bir yöneticiyi yalnızca kuruluş sahibi çıkarabilir.' })
    await this.db.$transaction([
      this.db.chatGroupMember.deleteMany({ where: { userId, group: { organizationId } } }),
      this.db.userPreference.deleteMany({ where: { userId, organizationId } }),
      this.db.organizationMember.delete({ where: { organizationId_userId: { organizationId, userId } } }),
    ])
    this.stream.emitToUsers([userId], 'organization:removed', { organizationId })
    // Kalan herkesin kişi listesi anında güncellensin; eskiden ancak uygulama
    // yeniden başlatılınca düşüyordu.
    this.stream.emit(organizationId, 'organization:member:removed', { organizationId, userId })
    return { userId, removed: true }
  }

  // Çalınan cihaz / paylaşılmış parola durumunda kişinin tüm oturumlarını düşürür.
  @Post('members/:userId/sessions/revoke')
  async revokeSessions(@Param('organizationId') organizationId: string, @Param('userId') userId: string, @Req() req: any) {
    const me = await this.staff(organizationId, req.user.sub, 'member.kick')
    const target = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!target) throw new NotFoundException({ code: 'MEMBER_NOT_FOUND', message: 'Üye bulunamadı.' })
    if (target.role === 'OWNER' && me.role !== 'OWNER') throw new ForbiddenException({ code: 'OWNER_PROTECTED', message: 'Kuruluş sahibinin oturumlarını yalnızca sahibi kapatabilir.' })
    const removed = await this.db.session.deleteMany({ where: { userId } })
    return { userId, closed: removed.count }
  }

  // Yetki kataloğu + her üyenin nereden hangi yetkiyi aldığı.
  @Get('permissions')
  async permissions(@Param('organizationId') organizationId: string, @Req() req: any) {
    await this.staff(organizationId, req.user.sub, 'member.roles')
    const members = await this.db.organizationMember.findMany({
      where: { organizationId }, orderBy: { joinedAt: 'asc' },
      select: { role: true, permissions: true, deniedPermissions: true, user: { select: { id: true, username: true, displayName: true, avatarUrl: true } } },
    })
    const rows = await Promise.all(members.map(async member => {
      const effective = await effectivePermissions(this.db, organizationId, member.user.id)
      return {
        ...member.user,
        owner: member.role === 'OWNER',
        roleIds: effective.roles.filter(role => !role.everyone).map(role => role.id),
        allow: cleanPermissions(member.permissions),
        deny: cleanPermissions(member.deniedPermissions),
        effective: effective.permissions,
      }
    }))
    const roles = await this.db.role.findMany({
      where: { organizationId },
      orderBy: [{ founder: 'desc' }, { everyone: 'desc' }, { handle: 'asc' }],
      select: {
        id: true, handle: true, name: true, color: true, description: true, everyone: true, founder: true, permissions: true,
        members: { select: { userId: true } },
        groups: { select: { group: { select: { id: true, handle: true, name: true, color: true, members: { select: { userId: true } } } } } },
      },
    })
    const groups = await this.db.userGroup.findMany({ where: { organizationId }, orderBy: { handle: 'asc' }, select: { id: true, handle: true, name: true, color: true, _count: { select: { members: true } } } })
    const total = members.length
    return {
      catalog: PERMISSIONS,
      members: rows,
      groups: groups.map(group => ({ id: group.id, handle: group.handle, name: group.name, color: group.color, memberCount: group._count.members })),
      roles: roles.map(role => {
        const holders = new Set<string>([...role.members.map(item => item.userId), ...role.groups.flatMap(item => item.group.members.map(member => member.userId))])
        return {
          id: role.id, handle: role.handle, name: role.name, color: role.color, description: role.description, everyone: role.everyone, founder: role.founder,
          permissions: cleanPermissions(role.permissions),
          memberCount: role.everyone ? total : holders.size,
          memberIds: role.everyone ? members.map(member => member.user.id) : role.members.map(member => member.userId),
          groups: role.groups.map(item => ({ id: item.group.id, handle: item.group.handle, name: item.group.name, color: item.group.color, memberCount: item.group.members.length })),
        }
      }),
    }
  }

  // Kişiye özel yetkiler: rolün üstüne eklenenler ve rolden/gruptan gelse de kapatılanlar.
  @Patch('members/:userId/permissions')
  async setPermissions(@Param('organizationId') organizationId: string, @Param('userId') userId: string, @Req() req: any, @Body() body: { allow?: unknown; deny?: unknown }) {
    await this.staff(organizationId, req.user.sub, 'member.roles')
    const target = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!target) throw new NotFoundException({ code: 'MEMBER_NOT_FOUND', message: 'Üye bulunamadı.' })
    if (target.role === 'OWNER') throw new ConflictException({ code: 'OWNER_PROTECTED', message: 'Kuruluş sahibinin yetkileri kısıtlanamaz.' })
    const allow = cleanPermissions(body?.allow)
    const deny = cleanPermissions(body?.deny).filter(item => !allow.includes(item))
    await this.db.organizationMember.update({ where: { organizationId_userId: { organizationId, userId } }, data: { permissions: allow, deniedPermissions: deny } })
    const effective = await effectivePermissions(this.db, organizationId, userId)
    this.stream.emitToUsers([userId], 'permissions:changed', { organizationId, permissions: effective.permissions })
    return { userId, allow, deny, effective: effective.permissions }
  }

  @Get('invites')
  async invites(@Param('organizationId') organizationId: string, @Req() req: any) {
    await this.staff(organizationId, req.user.sub, 'member.invite')
    return this.db.organizationInvite.findMany({
      where: { organizationId }, orderBy: { createdAt: 'desc' },
      select: { id: true, code: true, maxUses: true, useCount: true, expiresAt: true, createdAt: true, createdBy: { select: { id: true, displayName: true } } },
    })
  }

  @Post('invites')
  async createInvite(@Param('organizationId') organizationId: string, @Req() req: any, @Dto(InviteDto) dto: InviteDto) {
    await this.staff(organizationId, req.user.sub, 'member.invite')
    const code = `P2-${randomBytes(4).toString('hex').toUpperCase()}`
    return this.db.organizationInvite.create({
      data: {
        organizationId, code, createdById: req.user.sub,
        maxUses: dto.maxUses ?? null,
        expiresAt: dto.expiresInDays ? new Date(Date.now() + dto.expiresInDays * 86400000) : null,
      },
      select: { id: true, code: true, maxUses: true, useCount: true, expiresAt: true, createdAt: true, createdBy: { select: { id: true, displayName: true } } },
    })
  }

  // "Davet kodunu yenile": yeni kod üretir, eski varsayılan kodu geçersiz kılar.
  @Post('invites/rotate')
  async rotateInvite(@Param('organizationId') organizationId: string, @Req() req: any) {
    await this.staff(organizationId, req.user.sub, 'member.invite')
    const previous = await this.db.organizationInvite.findFirst({ where: { organizationId, maxUses: null, expiresAt: null }, orderBy: { createdAt: 'desc' } })
    const code = `P2-${randomBytes(4).toString('hex').toUpperCase()}`
    const invite = await this.db.organizationInvite.create({
      data: { organizationId, code, createdById: req.user.sub },
      select: { id: true, code: true, maxUses: true, useCount: true, expiresAt: true, createdAt: true, createdBy: { select: { id: true, displayName: true } } },
    })
    if (previous) await this.db.organizationInvite.delete({ where: { id: previous.id } }).catch(() => {})
    this.stream.emit(organizationId, 'organization:invite', { organizationId, inviteCode: code })
    return invite
  }

  @Delete('invites/:inviteId')
  async revokeInvite(@Param('organizationId') organizationId: string, @Param('inviteId') inviteId: string, @Req() req: any) {
    await this.staff(organizationId, req.user.sub, 'member.invite')
    const invite = await this.db.organizationInvite.findFirst({ where: { id: inviteId, organizationId } })
    if (!invite) throw new NotFoundException({ code: 'INVITE_NOT_FOUND', message: 'Davet bulunamadı.' })
    if ((await this.db.organizationInvite.count({ where: { organizationId } })) < 2) throw new ConflictException({ code: 'LAST_INVITE', message: 'Kuruluşun en az bir davet kodu olmalı.' })
    await this.db.organizationInvite.delete({ where: { id: inviteId } })
    return { id: inviteId, revoked: true }
  }

  @Get('channels')
  async channels(@Param('organizationId') organizationId: string, @Req() req: any) {
    await this.staff(organizationId, req.user.sub)
    const rows = await this.db.projectChannel.findMany({
      where: { category: { project: { organizationId } } },
      orderBy: [{ deletedAt: 'asc' }, { position: 'asc' }],
      select: { id: true, name: true, topic: true, type: true, deletedAt: true, purgeAt: true, createdAt: true, _count: { select: { messages: true } }, category: { select: { name: true, project: { select: { id: true, name: true } } } } },
    })
    return rows.map(row => ({
      id: row.id, name: row.name, topic: row.topic, type: row.type, deletedAt: row.deletedAt, purgeAt: row.purgeAt, createdAt: row.createdAt,
      messages: row._count.messages, category: row.category.name, project: row.category.project.name, projectId: row.category.project.id,
    }))
  }

  @Patch()
  async rename(@Param('organizationId') organizationId: string, @Req() req: any, @Dto(OrganizationDto) dto: OrganizationDto) {
    await this.staff(organizationId, req.user.sub, 'organization.manage')
    const name = dto.name?.trim()
    if (!name) throw new BadRequestException({ code: 'NAME_REQUIRED', message: 'Kuruluş adı boş olamaz.', field: 'name' })
    const organization = await this.db.organization.update({ where: { id: organizationId }, data: { name }, select: { id: true, name: true, slug: true } })
    this.stream.emit(organizationId, 'organization:updated', { organization })
    return organization
  }
}

// Arayüzün neyi göstereceğine karar verebilmesi için herkes kendi yetkisini okur.
@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/permissions')
export class PermissionsController {
  constructor(@Inject(PrismaService) private db: PrismaService) {}
  @Get('me')
  async me(@Param('organizationId') organizationId: string, @Req() req: any) {
    const effective = await effectivePermissions(this.db, organizationId, req.user.sub)
    // Kanal başına etkin yetkiler: arayüz hangi düğmeyi açacağını buradan bilir.
    const channels = await this.db.projectChannel.findMany({ where: { deletedAt: null, category: { project: { organizationId } } }, select: { id: true } })
    const overrides = await this.db.channelPermission.findMany({ where: { channelId: { in: channels.map(channel => channel.id) } } })
    const byChannel = new Map<string, typeof overrides>()
    for (const row of overrides) byChannel.set(row.channelId, [...(byChannel.get(row.channelId) || []), row])
    const perChannel: Record<string, string[]> = {}
    for (const channel of channels) perChannel[channel.id] = [...applyChannelOverrides(effective.permissions, effective, byChannel.get(channel.id) || [])]
    return { owner: effective.owner, permissions: effective.permissions, roles: effective.roles, allow: effective.allow, deny: effective.deny, channels: perChannel }
  }
}
