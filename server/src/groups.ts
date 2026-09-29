import { Body, Controller, Delete, ForbiddenException, Get, Inject, NotFoundException, Param, Patch, Post, Query, Req, UseGuards } from '@nestjs/common'
import { ArrayMaxSize, ArrayMinSize, IsArray, IsOptional, IsString, Length } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { groupNameOf, isAutoName } from './groupName'
import { mentionedUserIds, recordActivity } from './activity'
import { projectAudience, requirePermission, requireProjectAccess } from './permissions'

export class CreateGroupDto {
  @IsString() projectId!: string
  @IsOptional() @IsString() @Length(1, 60) name?: string
  @IsArray() @ArrayMinSize(1) @ArrayMaxSize(50) @IsString({ each: true }) userIds!: string[]
}
export class GroupMessageDto { @IsString() @Length(1, 4000) content!: string; @IsOptional() @IsString() quotedMessageId?: string }
export class GroupReplyDto { @IsString() @Length(1, 4000) content!: string }
export class ReactDto { @IsString() @Length(1, 24) emoji!: string }
export class AddMembersDto { @IsArray() @ArrayMinSize(1) @ArrayMaxSize(50) @IsString({ each: true }) userIds!: string[] }

const userSelect = { select: { id: true, username: true, displayName: true, status: true, avatarUrl: true } }
const messageSelect = {
  id: true, content: true, kind: true, createdAt: true, editedAt: true,
  author: { select: { id: true, username: true, displayName: true } },
  quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } },
  reactions: { select: { emoji: true, userId: true }, orderBy: { createdAt: 'asc' as const } },
  parentId: true,
  _count: { select: { replies: true } },
  // Thread kısayolunda son yanıtı verenin avatarı gösterilir.
  replies: { orderBy: { createdAt: 'desc' as const }, take: 3, select: { id: true, createdAt: true, author: { select: { id: true, displayName: true } } } },
}
// Tepkiler emoji başına toplanır: {emoji, count, reacted}
function groupReactions(rows: { emoji: string; userId: string }[], userId: string) {
  const map = new Map<string, { emoji: string; count: number; reacted: boolean }>()
  for (const row of rows) {
    const item = map.get(row.emoji) || { emoji: row.emoji, count: 0, reacted: false }
    item.count += 1
    if (row.userId === userId) item.reacted = true
    map.set(row.emoji, item)
  }
  return [...map.values()]
}

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/groups')
export class GroupsController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async membership(organizationId: string, userId: string) {
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
    return member
  }

  // Grup yalnızca üyelerine görünür; olaylar da yalnızca onların odalarına gider.
  private async group(organizationId: string, groupId: string, userId: string) {
    const found = await this.db.chatGroup.findFirst({
      where: { id: groupId, organizationId, members: { some: { userId } } },
      include: { members: { include: { user: userSelect } } },
    })
    if (!found) throw new NotFoundException({ code: 'GROUP_NOT_FOUND', message: 'Grup bulunamadı.' })
    if (!found.projectId) throw new NotFoundException({ code: 'GROUP_NOT_FOUND', message: 'Grup bulunamadı.' })
    await requireProjectAccess(this.db, organizationId, userId, found.projectId)
    return found
  }

  private shape(row: any, userId: string, unread = 0) {
    const me = row.members.find((member: any) => member.userId === userId)
    const last = row.messages?.[0]
    return {
      id: row.id,
      name: row.name,
      channelId: row.channelId,
      projectId: row.projectId,
      createdById: row.createdById,
      updatedAt: row.updatedAt,
      members: row.members.map((member: any) => member.user),
      lastReadAt: me?.lastReadAt ?? null,
      unread,
      lastMessage: last ? { id: last.id, content: last.content, createdAt: last.createdAt, authorId: last.authorId } : null,
    }
  }

  private unreadCount(groupId: string, userId: string, lastReadAt: Date | null) {
    return this.db.chatGroupMessage.count({ where: { groupId, authorId: { not: userId }, ...(lastReadAt ? { createdAt: { gt: lastReadAt } } : {}) } })
  }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Query('projectId') projectId: string | undefined, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    if (!projectId) return []
    await requireProjectAccess(this.db, organizationId, userId, projectId)
    const rows = await this.db.chatGroup.findMany({
      where: { organizationId, projectId, members: { some: { userId } } },
      orderBy: { updatedAt: 'desc' },
      include: {
        members: { include: { user: userSelect } },
        messages: { orderBy: { createdAt: 'desc' }, take: 1, select: { id: true, content: true, createdAt: true, authorId: true } },
      },
    })
    return Promise.all(rows.map(async row => {
      const me = row.members.find(member => member.userId === userId)
      return this.shape(row, userId, await this.unreadCount(row.id, userId, me?.lastReadAt ?? null))
    }))
  }

  @Post()
  async create(@Param('organizationId') organizationId: string, @Req() req: any, @Dto(CreateGroupDto) dto: CreateGroupDto) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    await requirePermission(this.db, organizationId, userId, 'group.create')
    await requireProjectAccess(this.db, organizationId, userId, dto.projectId)
    const others = [...new Set(dto.userIds.filter(id => id !== userId))]
    const ids = [userId, ...await projectAudience(this.db, organizationId, dto.projectId, others)]
    const people = await this.db.user.findMany({ where: { id: { in: ids } }, select: { id: true, displayName: true } })
    const name = dto.name?.trim() || groupNameOf(people.map(person => person.displayName))

    const group = await this.db.chatGroup.create({
      data: { organizationId, projectId: dto.projectId, name, createdById: userId, members: { create: ids.map(id => ({ userId: id })) } },
      include: { members: { include: { user: userSelect } } },
    })
    const payload = this.shape({ ...group, messages: [] }, userId)
    this.stream.emitToUsers(ids, 'group:created', { group: payload })
    return payload
  }

  @Post(':groupId/members')
  async addMembers(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any, @Dto(AddMembersDto) dto: AddMembersDto) {
    const userId = req.user.sub as string
    const group = await this.group(organizationId, groupId, userId)
    const existing = new Set(group.members.map(member => member.userId))
    const allowed = group.projectId ? await projectAudience(this.db, organizationId, group.projectId, dto.userIds.filter(id => !existing.has(id))) : []
    const invited = allowed.map(userId => ({ userId }))
    if (invited.length) {
      await this.db.chatGroupMember.createMany({ data: invited.map(member => ({ groupId, userId: member.userId })), skipDuplicates: true })
    }
    if (invited.length) {
      await this.stream.inviteToGroupCall(groupId, invited.map(member => member.userId))
      // Sohbete "eklendi" satırı düşer.
      const names = await this.db.user.findMany({ where: { id: { in: invited.map(member => member.userId) } }, select: { displayName: true } })
      const notice = await this.db.chatGroupMessage.create({
        data: { groupId, authorId: userId, kind: 'SYSTEM', content: `${names.map(person => person.displayName).join(', ')} gruba eklendi.` },
        select: messageSelect,
      })
      const audience = await this.db.chatGroupMember.findMany({ where: { groupId }, select: { userId: true } })
      this.stream.emitToUsers(audience.map(member => member.userId), 'group:message', { groupId, message: notice })
    }
    let fresh = await this.group(organizationId, groupId, userId)
    // Ad elle verilmediyse (eski üye listesinden üretilmişse) yeni listeyle tazelenir.
    if (isAutoName(group.name, group.members.map(member => member.user.displayName))) {
      await this.db.chatGroup.update({ where: { id: groupId }, data: { name: groupNameOf(fresh.members.map(member => member.user.displayName)) } })
      fresh = await this.group(organizationId, groupId, userId)
    }
    const payload = this.shape({ ...fresh, messages: [] }, userId)
    const added = invited.map(member => member.userId)
    // Yeni eklenenler grubu listelerinde göremiyordu: onlara "oluşturuldu" gider.
    if (added.length) this.stream.emitToUsers(added, 'group:created', { group: payload })
    this.stream.emitToUsers(fresh.members.map(member => member.userId).filter(id => !added.includes(id)), 'group:updated', { group: payload })
    return payload
  }

  // Gruptan çıkarma: yalnızca grubu kuran kişi yapabilir.
  @Delete(':groupId/members/:memberId')
  async removeMember(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Param('memberId') memberId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const group = await this.group(organizationId, groupId, userId)
    if (group.createdById !== userId) throw new ForbiddenException({ code: 'NOT_OWNER', message: 'Yalnızca grubu kuran kişi üye çıkarabilir.' })
    if (memberId === userId) throw new ForbiddenException({ code: 'NOT_SELF', message: 'Kendini çıkaramazsın; gruptan ayrıl.' })
    const target = group.members.find(member => member.userId === memberId)
    if (!target) throw new NotFoundException({ code: 'MEMBER_NOT_FOUND', message: 'Üye bulunamadı.' })

    await this.db.chatGroupMember.delete({ where: { groupId_userId: { groupId, userId: memberId } } })
    await this.stream.dropFromGroupCall(groupId, memberId)
    this.stream.emitToUsers([memberId], 'group:left', { groupId })

    const remaining = group.members.filter(member => member.userId !== memberId)
    if (isAutoName(group.name, group.members.map(member => member.user.displayName))) {
      await this.db.chatGroup.update({ where: { id: groupId }, data: { name: groupNameOf(remaining.map(member => member.user.displayName)) } })
    }
    const message = await this.db.chatGroupMessage.create({
      data: { groupId, authorId: userId, kind: 'SYSTEM', content: `${target.user.displayName} gruptan çıkarıldı.` },
      select: messageSelect,
    })
    const fresh = await this.db.chatGroup.findFirst({ where: { id: groupId }, include: { members: { include: { user: userSelect } } } })
    if (fresh) {
      const audience = remaining.map(member => member.userId)
      this.stream.emitToUsers(audience, 'group:message', { groupId, message })
      this.stream.emitToUsers(audience, 'group:updated', { group: this.shape({ ...fresh, messages: [] }, userId) })
    }
    return { ok: true }
  }

  // Gruptan ayrılma: kalanlara sistem mesajı ve güncel liste gider.
  @Post(':groupId/leave')
  async leave(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const group = await this.group(organizationId, groupId, userId)
    const me = group.members.find(member => member.userId === userId)
    const remaining = group.members.filter(member => member.userId !== userId)
    await this.db.chatGroupMember.delete({ where: { groupId_userId: { groupId, userId } } })
    await this.stream.dropFromGroupCall(groupId, userId)
    this.stream.emitToUsers([userId], 'group:left', { groupId })
    if (!remaining.length) {
      await this.db.chatGroup.delete({ where: { id: groupId } })
      return { ok: true }
    }
    const message = await this.db.chatGroupMessage.create({
      data: { groupId, authorId: userId, kind: 'SYSTEM', content: `${me?.user.displayName || 'Bir üye'} gruptan ayrıldı.` },
      select: messageSelect,
    })
    // Ad otomatikse ayrılan kişi adından da düşer.
    if (isAutoName(group.name, group.members.map(member => member.user.displayName))) {
      await this.db.chatGroup.update({ where: { id: groupId }, data: { name: groupNameOf(remaining.map(member => member.user.displayName)) } })
    }
    const fresh = await this.db.chatGroup.findFirst({ where: { id: groupId }, include: { members: { include: { user: userSelect } } } })
    if (fresh) {
      const payload = this.shape({ ...fresh, messages: [] }, remaining[0].userId)
      this.stream.emitToUsers(remaining.map(member => member.userId), 'group:message', { groupId, message })
      this.stream.emitToUsers(remaining.map(member => member.userId), 'group:updated', { group: payload })
    }
    return { ok: true }
  }

  @Get(':groupId/messages')
  async messages(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.group(organizationId, groupId, userId)
    const rows = await this.db.chatGroupMessage.findMany({ where: { groupId, parentId: null }, orderBy: { createdAt: 'asc' }, take: 200, select: messageSelect })
    return rows
  }

  @Post(':groupId/messages')
  async send(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any, @Dto(GroupMessageDto) dto: GroupMessageDto) {
    const userId = req.user.sub as string
    const group = await this.group(organizationId, groupId, userId)
    const quoted = dto.quotedMessageId ? await this.db.chatGroupMessage.findFirst({ where: { id: dto.quotedMessageId, groupId }, select: { id: true } }) : null
    const message = await this.db.chatGroupMessage.create({ data: { groupId, authorId: userId, content: dto.content, quotedMessageId: quoted?.id }, select: messageSelect })
    await this.db.chatGroup.update({ where: { id: groupId }, data: { updatedAt: new Date() } })
    this.stream.emitToUsers(group.members.map(member => member.userId), 'group:message', { groupId, message })
    await this.notifyGroupMentions(organizationId, userId, dto.content, group, message.id, null)
    return message
  }

  // Grupta @kullanıcı adı geçenlere etkinlik kaydı düşer (yalnızca grup üyeleri).
  private async notifyGroupMentions(organizationId: string, actorId: string, content: string, group: { id: string; name: string; members: { userId: string; user: { username: string } }[] }, messageId: string, parentId: string | null) {
    // Kullanıcı grupları da (@devs) çözülür ama bildirim yine yalnızca sohbet
    // grubundaki kişilere gider; gruba dahil olmayan kimse haberdar edilmez.
    const mentioned = new Set(await mentionedUserIds(this.db, organizationId, content))
    if (!mentioned.size) return
    const targets = group.members.filter(member => mentioned.has(member.userId))
    await recordActivity(this.db, this.stream, targets.map(member => ({
      organizationId, userId: member.userId, actorId, kind: 'MENTION' as const, preview: content,
      groupId: group.id, groupName: group.name, messageId, parentId,
    })))
  }

  @Get(':groupId/messages/:messageId/replies')
  async replies(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Param('messageId') messageId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.group(organizationId, groupId, userId)
    return this.db.chatGroupMessage.findMany({ where: { groupId, parentId: messageId }, orderBy: { createdAt: 'asc' }, select: messageSelect })
  }

  @Post(':groupId/messages/:messageId/replies')
  async reply(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Param('messageId') messageId: string, @Req() req: any, @Dto(GroupReplyDto) dto: GroupReplyDto) {
    const userId = req.user.sub as string
    const group = await this.group(organizationId, groupId, userId)
    const parent = await this.db.chatGroupMessage.findFirst({ where: { id: messageId, groupId, parentId: null } })
    if (!parent) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    const message = await this.db.chatGroupMessage.create({ data: { groupId, authorId: userId, content: dto.content, parentId: messageId }, select: messageSelect })
    await this.db.chatGroup.update({ where: { id: groupId }, data: { updatedAt: new Date() } })
    const replies = await this.db.chatGroupMessage.count({ where: { parentId: messageId } })
    this.stream.emitToUsers(group.members.map(member => member.userId), 'group:reply', { groupId, parentId: messageId, message, replies })
    await recordActivity(this.db, this.stream, [{
      organizationId, userId: parent.authorId, actorId: userId, kind: 'REPLY', preview: dto.content,
      groupId: group.id, groupName: group.name, messageId: message.id, parentId: messageId,
    }])
    await this.notifyGroupMentions(organizationId, userId, dto.content, group, message.id, messageId)
    return message
  }

  @Patch(':groupId/messages/:messageId')
  async edit(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Param('messageId') messageId: string, @Req() req: any, @Dto(GroupMessageDto) dto: GroupMessageDto) {
    const userId = req.user.sub as string
    const group = await this.group(organizationId, groupId, userId)
    const existing = await this.db.chatGroupMessage.findFirst({ where: { id: messageId, groupId } })
    if (!existing) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    if (existing.authorId !== userId) throw new ForbiddenException({ code: 'NOT_AUTHOR', message: 'Yalnızca kendi mesajını düzenleyebilirsin.' })
    const message = await this.db.chatGroupMessage.update({ where: { id: messageId }, data: { content: dto.content, editedAt: new Date() }, select: messageSelect })
    this.stream.emitToUsers(group.members.map(member => member.userId), 'group:message:updated', { groupId, message })
    return message
  }

  @Delete(':groupId/messages/:messageId')
  async remove(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Param('messageId') messageId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const group = await this.group(organizationId, groupId, userId)
    const existing = await this.db.chatGroupMessage.findFirst({ where: { id: messageId, groupId } })
    if (!existing) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    if (existing.authorId !== userId) throw new ForbiddenException({ code: 'NOT_AUTHOR', message: 'Yalnızca kendi mesajını silebilirsin.' })
    await this.db.chatGroupMessage.delete({ where: { id: messageId } })
    this.stream.emitToUsers(group.members.map(member => member.userId), 'group:message:deleted', { groupId, messageId })
    return { ok: true }
  }

  @Post(':groupId/messages/:messageId/reactions')
  async react(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Param('messageId') messageId: string, @Req() req: any, @Dto(ReactDto) dto: ReactDto) {
    const userId = req.user.sub as string
    const group = await this.group(organizationId, groupId, userId)
    const target = await this.db.chatGroupMessage.findFirst({ where: { id: messageId, groupId }, select: { id: true, authorId: true, content: true, parentId: true } })
    if (!target) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    const emoji = dto.emoji.trim()
    const existing = await this.db.chatReaction.findFirst({ where: { groupMessageId: messageId, userId, emoji } })
    if (existing) await this.db.chatReaction.delete({ where: { id: existing.id } })
    else {
      await this.db.chatReaction.create({ data: { groupMessageId: messageId, userId, emoji } })
      await recordActivity(this.db, this.stream, [{
        organizationId, userId: target.authorId, actorId: userId, kind: 'REACTION', preview: target.content, emoji,
        groupId: group.id, groupName: group.name, messageId: target.id, parentId: target.parentId,
      }])
    }
    const rows = await this.db.chatReaction.findMany({ where: { groupMessageId: messageId }, orderBy: { createdAt: 'asc' }, select: { emoji: true, userId: true } })
    this.stream.emitToUsers(group.members.map(member => member.userId), 'group:reactions', { groupId, messageId, rows })
    return groupReactions(rows, userId)
  }

  @Post(':groupId/read')
  async read(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.group(organizationId, groupId, userId)
    await this.db.chatGroupMember.update({ where: { groupId_userId: { groupId, userId } }, data: { lastReadAt: new Date() } })
    return { ok: true }
  }
}
