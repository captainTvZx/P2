import { Body, Controller, Delete, Get, Inject, NotFoundException, Param, Put, Req, UseGuards } from '@nestjs/common'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { channelPermissions, cleanChannelPermissions, CHANNEL_PERMISSIONS, PERMISSIONS, requireChannelPermission } from './permissions'

// Kanala özel izinler: rol, grup ya da tek kişi için yetki aç/kapat. Kurallar
// herkes rolü → diğer roller → gruplar → kişi sırasıyla uygulanır; en dar kural kazanır.
@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects/:projectId/channels/:channelId/permissions')
export class ChannelPermissionsController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async channel(organizationId: string, projectId: string, channelId: string) {
    const channel = await this.db.projectChannel.findFirst({ where: { id: channelId, deletedAt: null, category: { projectId, project: { organizationId } } }, select: { id: true, name: true } })
    if (!channel) throw new NotFoundException({ code: 'CHANNEL_NOT_FOUND', message: 'Kanal bulunamadı.' })
    return channel
  }

  private async announce(organizationId: string, projectId: string, channelId: string) {
    // Yetkiler değişince herkesin kanal listesi ve düğmeleri tazelenmeli.
    this.stream.emit(organizationId, 'channel:permissions', { projectId, channelId })
  }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string, @Req() req: any) {
    await this.channel(organizationId, projectId, channelId)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.view')
    const [rows, roles, groups, members] = await Promise.all([
      this.db.channelPermission.findMany({ where: { channelId }, orderBy: { updatedAt: 'asc' } }),
      this.db.role.findMany({ where: { organizationId, founder: false }, orderBy: [{ everyone: 'desc' }, { handle: 'asc' }], select: { id: true, handle: true, name: true, color: true, everyone: true } }),
      this.db.userGroup.findMany({ where: { organizationId }, orderBy: { handle: 'asc' }, select: { id: true, handle: true, name: true, color: true, _count: { select: { members: true } } } }),
      this.db.organizationMember.findMany({ where: { organizationId }, orderBy: { joinedAt: 'asc' }, select: { role: true, user: { select: { id: true, username: true, displayName: true, avatarUrl: true } } } }),
    ])
    const mine = await channelPermissions(this.db, organizationId, req.user.sub, channelId)
    return {
      catalog: PERMISSIONS.filter(item => (CHANNEL_PERMISSIONS as string[]).includes(item.id)),
      overrides: rows.map(row => ({ targetType: row.targetType, targetId: row.targetId, allow: cleanChannelPermissions(row.allow), deny: cleanChannelPermissions(row.deny) })),
      roles,
      groups: groups.map(group => ({ id: group.id, handle: group.handle, name: group.name, color: group.color, memberCount: group._count.members })),
      members: members.map(row => ({ ...row.user, owner: row.role === 'OWNER' })),
      mine: [...mine.permissions],
      canEdit: mine.permissions.has('channel.permissions'),
    }
  }

  // Tek bir hedefin (rol/grup/kişi) kuralını yazar; ikisi de boşsa kural silinir.
  @Put(':targetType/:targetId')
  async save(
    @Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string,
    @Param('targetType') targetType: string, @Param('targetId') targetId: string,
    @Req() req: any, @Body() body: { allow?: unknown; deny?: unknown },
  ) {
    await this.channel(organizationId, projectId, channelId)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.permissions')
    const type = String(targetType).toUpperCase()
    if (type !== 'ROLE' && type !== 'GROUP' && type !== 'USER') throw new NotFoundException({ code: 'TARGET_INVALID', message: 'Geçersiz hedef.' })
    if (type === 'ROLE' && !(await this.db.role.findFirst({ where: { id: targetId, organizationId, founder: false } }))) throw new NotFoundException({ code: 'ROLE_NOT_FOUND', message: 'Rol bulunamadı ya da kural konulamaz.' })
    if (type === 'GROUP' && !(await this.db.userGroup.findFirst({ where: { id: targetId, organizationId } }))) throw new NotFoundException({ code: 'USER_GROUP_NOT_FOUND', message: 'Grup bulunamadı.' })
    if (type === 'USER' && !(await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId: targetId } } }))) throw new NotFoundException({ code: 'MEMBER_NOT_FOUND', message: 'Üye bulunamadı.' })

    const allow = cleanChannelPermissions(body?.allow)
    const deny = cleanChannelPermissions(body?.deny).filter(item => !allow.includes(item))
    if (!allow.length && !deny.length) {
      await this.db.channelPermission.deleteMany({ where: { channelId, targetType: type, targetId } })
      await this.announce(organizationId, projectId, channelId)
      return { targetType: type, targetId, allow: [], deny: [], removed: true }
    }
    const row = await this.db.channelPermission.upsert({
      where: { channelId_targetType_targetId: { channelId, targetType: type, targetId } },
      create: { channelId, targetType: type, targetId, allow, deny },
      update: { allow, deny },
    })
    await this.announce(organizationId, projectId, channelId)
    return { targetType: row.targetType, targetId: row.targetId, allow, deny }
  }

  @Delete(':targetType/:targetId')
  async remove(
    @Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('channelId') channelId: string,
    @Param('targetType') targetType: string, @Param('targetId') targetId: string, @Req() req: any,
  ) {
    await this.channel(organizationId, projectId, channelId)
    await requireChannelPermission(this.db, organizationId, req.user.sub, channelId, 'channel.permissions')
    await this.db.channelPermission.deleteMany({ where: { channelId, targetType: String(targetType).toUpperCase(), targetId } })
    await this.announce(organizationId, projectId, channelId)
    return { removed: true }
  }
}
