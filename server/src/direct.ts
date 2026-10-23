import { ConflictException, Controller, Delete, Get, Inject, NotFoundException, Param, Patch, Post, Query, Req, UseGuards } from '@nestjs/common'
import { IsOptional, IsString, Length } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { effectivePermissions, requirePermission } from './permissions'
import { pageArgs } from './paging'

export class StartConversationDto { @IsString() userId!: string }
export class DirectMessageDto { @IsString() @Length(1, 4000) content!: string; @IsOptional() @IsString() quotedMessageId?: string }
export class DirectReactDto { @IsString() @Length(1, 24) emoji!: string }

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

const authorSelect = { select: { id: true, username: true, displayName: true } }
const messageSelect = { id: true, content: true, kind: true, createdAt: true, editedAt: true, author: authorSelect, quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } }, reactions: { select: { emoji: true, userId: true }, orderBy: { createdAt: 'asc' as const } } }
const userSelect = { select: { id: true, username: true, displayName: true, status: true } }
const lastMessageSelect = { orderBy: { createdAt: 'desc' as const }, take: 1, select: { id: true, content: true, createdAt: true, authorId: true } }

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/direct')
export class DirectController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async membership(organizationId: string, userId: string) {
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
    return member
  }

  private async conversation(organizationId: string, conversationId: string, userId: string) {
    const found = await this.db.directConversation.findFirst({ where: { id: conversationId, organizationId, OR: [{ userAId: userId }, { userBId: userId }] }, include: { userA: userSelect, userB: userSelect } })
    if (!found) throw new NotFoundException({ code: 'CONVERSATION_NOT_FOUND', message: 'Sohbet bulunamadı.' })
    return found
  }

  // Her iki katılımcı da kendi "user:{id}" odasında olduğu için olaylar yalnızca onlara gider.
  private notify(conversation: { userAId: string; userBId: string }, event: string, payload: unknown) {
    this.stream.emitToUsers([conversation.userAId, conversation.userBId], event, payload)
  }

  private shape(row: any, userId: string, unread = 0) {
    const other = row.userAId === userId ? row.userB : row.userA
    const lastReadAt = row.userAId === userId ? row.userALastReadAt : row.userBLastReadAt
    const last = row.messages?.[0]
    return { id: row.id, user: other, updatedAt: row.updatedAt, lastReadAt, unread, lastMessage: last ? { id: last.id, content: last.content, createdAt: last.createdAt, authorId: last.authorId } : null }
  }

  private unreadCount(conversationId: string, userId: string, lastReadAt: Date | null) {
    return this.db.directMessage.count({ where: { conversationId, authorId: { not: userId }, ...(lastReadAt ? { createdAt: { gt: lastReadAt } } : {}) } })
  }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    const rows = await this.db.directConversation.findMany({
      where: { organizationId, OR: [{ userAId: userId }, { userBId: userId }] },
      orderBy: { updatedAt: 'desc' },
      include: { userA: userSelect, userB: userSelect, messages: lastMessageSelect },
    })
    return Promise.all(rows.map(async row => this.shape(row, userId, await this.unreadCount(row.id, userId, row.userAId === userId ? row.userALastReadAt : row.userBLastReadAt))))
  }

  @Post()
  async start(@Param('organizationId') organizationId: string, @Req() req: any, @Dto(StartConversationDto) dto: StartConversationDto) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    if (dto.userId === userId) throw new ConflictException({ code: 'SELF_CONVERSATION', message: 'Kendinle sohbet başlatamazsın.' })
    await this.membership(organizationId, dto.userId)
    const [userAId, userBId] = [userId, dto.userId].sort()
    const existing = await this.db.directConversation.findUnique({ where: { organizationId_userAId_userBId: { organizationId, userAId, userBId } }, include: { userA: userSelect, userB: userSelect, messages: lastMessageSelect } })
    if (existing) return this.shape(existing, userId, await this.unreadCount(existing.id, userId, existing.userAId === userId ? existing.userALastReadAt : existing.userBLastReadAt))
    const created = await this.db.directConversation.create({ data: { organizationId, userAId, userBId }, include: { userA: userSelect, userB: userSelect, messages: lastMessageSelect } })
    this.notify(created, 'direct:conversation', { conversationId: created.id })
    return this.shape(created, userId)
  }

  // Sayfalama: ?before=<ISO createdAt>&limit=<1..200>; parametresiz çağrı en yeni 200 mesajı artan sırada döner.
  @Get(':conversationId/messages')
  async messages(@Param('organizationId') organizationId: string, @Param('conversationId') conversationId: string, @Query('before') before: string | undefined, @Query('limit') limit: string | undefined, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    await this.conversation(organizationId, conversationId, userId)
    const page = pageArgs(before, limit, { take: 200, max: 200 })
    const rows = await this.db.directMessage.findMany({ where: { conversationId, ...page.where }, orderBy: { createdAt: 'desc' }, take: page.take, select: messageSelect })
    return rows.reverse()
  }

  @Post(':conversationId/messages')
  async send(@Param('organizationId') organizationId: string, @Param('conversationId') conversationId: string, @Req() req: any, @Dto(DirectMessageDto) dto: DirectMessageDto) {
    await requirePermission(this.db, organizationId, req.user.sub, 'direct.send')
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    const conversation = await this.conversation(organizationId, conversationId, userId)
    const now = new Date()
    if (dto.quotedMessageId && !(await this.db.directMessage.findFirst({ where: { id: dto.quotedMessageId, conversationId } }))) throw new NotFoundException({ code: 'QUOTED_MESSAGE_NOT_FOUND', message: 'Alıntılanan mesaj bulunamadı.' })
    const message = await this.db.directMessage.create({ data: { conversationId, authorId: userId, content: dto.content.trim(), quotedMessageId: dto.quotedMessageId || null }, select: messageSelect })
    await this.db.directConversation.update({ where: { id: conversationId }, data: { updatedAt: now, ...(conversation.userAId === userId ? { userALastReadAt: now } : { userBLastReadAt: now }) } })
    this.notify(conversation, 'direct:created', { conversationId, message })
    return message
  }

  @Patch(':conversationId/messages/:messageId')
  async edit(@Param('organizationId') organizationId: string, @Param('conversationId') conversationId: string, @Param('messageId') messageId: string, @Req() req: any, @Dto(DirectMessageDto) dto: DirectMessageDto) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    const conversation = await this.conversation(organizationId, conversationId, userId)
    if (!(await this.db.directMessage.findFirst({ where: { id: messageId, conversationId, authorId: userId } }))) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    const message = await this.db.directMessage.update({ where: { id: messageId }, data: { content: dto.content.trim(), editedAt: new Date() }, select: messageSelect })
    this.notify(conversation, 'direct:updated', { conversationId, message })
    return message
  }

  @Delete(':conversationId/messages/:messageId')
  async remove(@Param('organizationId') organizationId: string, @Param('conversationId') conversationId: string, @Param('messageId') messageId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    const conversation = await this.conversation(organizationId, conversationId, userId)
    const message = await this.db.directMessage.findFirst({ where: { id: messageId, conversationId }, select: { authorId: true } })
    if (!message) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    if (message.authorId !== userId && !(await effectivePermissions(this.db, organizationId, userId)).permissions.includes('message.delete_unlimited')) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    await this.db.directMessage.delete({ where: { id: messageId } })
    this.notify(conversation, 'direct:deleted', { conversationId, messageId })
    return { deleted: true }
  }

  @Post(':conversationId/messages/:messageId/reactions')
  async react(@Param('organizationId') organizationId: string, @Param('conversationId') conversationId: string, @Param('messageId') messageId: string, @Req() req: any, @Dto(DirectReactDto) dto: DirectReactDto) {
    const userId = req.user.sub as string
    const conversation = await this.conversation(organizationId, conversationId, userId)
    if (!(await this.db.directMessage.findFirst({ where: { id: messageId, conversationId } }))) throw new NotFoundException({ code: 'MESSAGE_NOT_FOUND', message: 'Mesaj bulunamadı.' })
    const emoji = dto.emoji.trim()
    const existing = await this.db.chatReaction.findFirst({ where: { directMessageId: messageId, userId, emoji } })
    if (existing) await this.db.chatReaction.delete({ where: { id: existing.id } })
    else await this.db.chatReaction.create({ data: { directMessageId: messageId, userId, emoji } })
    const rows = await this.db.chatReaction.findMany({ where: { directMessageId: messageId }, orderBy: { createdAt: 'asc' }, select: { emoji: true, userId: true } })
    this.notify(conversation, 'direct:reactions', { conversationId, messageId, rows })
    return groupReactions(rows, userId)
  }

  @Post(':conversationId/read')
  async read(@Param('organizationId') organizationId: string, @Param('conversationId') conversationId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    const conversation = await this.conversation(organizationId, conversationId, userId)
    const lastReadAt = new Date()
    await this.db.directConversation.update({ where: { id: conversationId }, data: conversation.userAId === userId ? { userALastReadAt: lastReadAt } : { userBLastReadAt: lastReadAt } })
    return { lastReadAt }
  }
}
