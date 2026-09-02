import { Body, Controller, Get, Inject, NotFoundException, Param, Post, Req, UseGuards } from '@nestjs/common'
import { ArrayMaxSize, ArrayMinSize, IsArray, IsOptional, IsString, Length } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'

export class CreateGroupDto {
  @IsOptional() @IsString() @Length(1, 60) name?: string
  @IsArray() @ArrayMinSize(1) @ArrayMaxSize(50) @IsString({ each: true }) userIds!: string[]
}
export class GroupMessageDto { @IsString() @Length(1, 4000) content!: string }
export class AddMembersDto { @IsArray() @ArrayMinSize(1) @ArrayMaxSize(50) @IsString({ each: true }) userIds!: string[] }

const userSelect = { select: { id: true, username: true, displayName: true, status: true } }
const messageSelect = { id: true, content: true, kind: true, createdAt: true, editedAt: true, author: { select: { id: true, username: true, displayName: true } } }

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
    const name = dto.name?.trim() || people.filter(person => person.id !== userId).map(person => person.displayName).join(', ').slice(0, 60) || 'Yeni grup'

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
    const fresh = await this.group(organizationId, groupId, userId)
    const payload = this.shape({ ...fresh, messages: [] }, userId)
    this.stream.emitToUsers(fresh.members.map(member => member.userId), 'group:updated', { group: payload })
    return payload
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
    const message = await this.db.chatGroupMessage.create({ data: { groupId, authorId: userId, content: dto.content }, select: messageSelect })
    await this.db.chatGroup.update({ where: { id: groupId }, data: { updatedAt: new Date() } })
    this.stream.emitToUsers(group.members.map(member => member.userId), 'group:message', { groupId, message })
    return message
  }

  @Post(':groupId/read')
  async read(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.group(organizationId, groupId, userId)
    await this.db.chatGroupMember.update({ where: { groupId_userId: { groupId, userId } }, data: { lastReadAt: new Date() } })
    return { ok: true }
  }
}
