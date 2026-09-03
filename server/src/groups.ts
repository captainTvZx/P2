import { Body, Controller, Delete, ForbiddenException, Get, Inject, NotFoundException, Param, Patch, Post, Req, UseGuards } from '@nestjs/common'
import { ArrayMaxSize, ArrayMinSize, IsArray, IsOptional, IsString, Length } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { groupNameOf, isAutoName } from './groupName'

export class CreateGroupDto {
  @IsOptional() @IsString() @Length(1, 60) name?: string
  @IsArray() @ArrayMinSize(1) @ArrayMaxSize(50) @IsString({ each: true }) userIds!: string[]
}
export class GroupMessageDto { @IsString() @Length(1, 4000) content!: string; @IsOptional() @IsString() quotedMessageId?: string }
export class ReactDto { @IsString() @Length(1, 24) emoji!: string }
export class AddMembersDto { @IsArray() @ArrayMinSize(1) @ArrayMaxSize(50) @IsString({ each: true }) userIds!: string[] }

const userSelect = { select: { id: true, username: true, displayName: true, status: true } }
const messageSelect = {
  id: true, content: true, kind: true, createdAt: true, editedAt: true,
  author: { select: { id: true, username: true, displayName: true } },
  quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } },
  reactions: { select: { emoji: true, userId: true }, orderBy: { createdAt: 'asc' as const } },
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
    return found
  }

  private shape(row: any, userId: string, unread = 0) {
    const me = row.members.find((member: any) => member.userId === userId)
    const last = row.messages?.[0]
    return {
      id: row.id,
      name: row.name,
      channelId: row.channelId,
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
  async list(@Param('organizationId') organizationId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    const rows = await this.db.chatGroup.findMany({
      where: { organizationId, members: { some: { userId } } },
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
    const others = [...new Set(dto.userIds.filter(id => id !== userId))]
    const members = await this.db.organizationMember.findMany({ where: { organizationId, userId: { in: others } }, select: { userId: true } })
    const ids = [userId, ...members.map(member => member.userId)]
    const people = await this.db.user.findMany({ where: { id: { in: ids } }, select: { id: true, displayName: true } })
    const name = dto.name?.trim() || groupNameOf(people.map(person => person.displayName))

    const group = await this.db.chatGroup.create({
      data: { organizationId, name, createdById: userId, members: { create: ids.map(id => ({ userId: id })) } },
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
    const invited = await this.db.organizationMember.findMany({
      where: { organizationId, userId: { in: dto.userIds.filter(id => !existing.has(id)) } },
      select: { userId: true },
    })
    if (invited.length) {
      await this.db.chatGroupMember.createMany({ data: invited.map(member => ({ groupId, userId: member.userId })), skipDuplicates: true })
    }
    if (invited.length) await this.stream.inviteToGroupCall(groupId, invited.map(member => member.userId))
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
    const rows = await this.db.chatGroupMessage.findMany({ where: { groupId }, orderBy: { createdAt: 'asc' }, take: 200, select: messageSelect })
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
    if (!(await this.db.chatGroupMessage.findFirst({ where: { id: messageId, groupId } }))) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    const emoji = dto.emoji.trim()
    const existing = await this.db.chatReaction.findFirst({ where: { groupMessageId: messageId, userId, emoji } })
    if (existing) await this.db.chatReaction.delete({ where: { id: existing.id } })
    else await this.db.chatReaction.create({ data: { groupMessageId: messageId, userId, emoji } })
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
