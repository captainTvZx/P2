import { Controller, Get, Inject, NotFoundException, Param, Post, Query, Req, UseGuards } from '@nestjs/common'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'

export type ActivityKind = 'MENTION' | 'REPLY' | 'REACTION'
export type ActivityInput = {
  organizationId: string
  userId: string          // kime ait
  actorId: string         // kim yaptı
  kind: ActivityKind
  preview: string
  projectId?: string | null
  channelId?: string | null
  channelName?: string | null
  groupId?: string | null
  groupName?: string | null
  messageId?: string | null
  parentId?: string | null
  emoji?: string | null
}

const actorSelect = { select: { id: true, username: true, displayName: true } }

// Metindeki @kullanıcı adlarını çıkarır (Türkçe karakter içermeyen kullanıcı adları).
export function mentionedUsernames(content: string) {
  return [...new Set((content.match(/@([a-zA-Z0-9_.-]+)/g) || []).map(item => item.slice(1).toLocaleLowerCase('tr-TR')))]
}

// Etiketlenen kişiler: doğrudan @kullanıcı adları + @devs gibi kullanıcı
// gruplarının üyeleri. Grup etiketi tek seferde bütün ekibi haberdar eder.
export async function mentionedUserIds(db: PrismaService, organizationId: string, content: string) {
  const names = mentionedUsernames(content)
  if (!names.length) return []
  const [members, groups] = await Promise.all([
    db.organizationMember.findMany({ where: { organizationId, user: { username: { in: names, mode: 'insensitive' } } }, select: { userId: true } }),
    db.userGroup.findMany({ where: { organizationId, handle: { in: names } }, select: { members: { select: { userId: true } } } }),
  ])
  return [...new Set([...members.map(member => member.userId), ...groups.flatMap(group => group.members.map(member => member.userId))])]
}

export function previewOf(content: string) {
  return content.replace(/\s+/g, ' ').trim().slice(0, 140)
}

// Etkinlikleri yazar ve ilgili kişilere canlı olarak bildirir. Kişinin kendi
// eylemleri kaydedilmez.
export async function recordActivity(db: PrismaService, stream: RealtimeGateway, inputs: ActivityInput[]) {
  const rows = inputs.filter(item => item.userId && item.userId !== item.actorId)
  if (!rows.length) return
  try {
    for (const row of rows) {
      const created = await db.activity.create({
        data: {
          organizationId: row.organizationId,
          userId: row.userId,
          actorId: row.actorId,
          kind: row.kind,
          preview: previewOf(row.preview),
          projectId: row.projectId ?? null,
          channelId: row.channelId ?? null,
          channelName: row.channelName ?? null,
          groupId: row.groupId ?? null,
          groupName: row.groupName ?? null,
          messageId: row.messageId ?? null,
          parentId: row.parentId ?? null,
          emoji: row.emoji ?? null,
        },
        include: { actor: actorSelect },
      })
      stream.emitToUsers([row.userId], 'activity:new', { activity: created })
    }
  } catch {}
}

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/activity')
export class ActivityController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async membership(organizationId: string, userId: string) {
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
    return member
  }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Query('limit') limit: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    const take = Math.min(Math.max(Number(limit) || 60, 1), 200)
    const rows = await this.db.activity.findMany({
      where: { organizationId, userId },
      orderBy: { createdAt: 'desc' },
      take,
      include: { actor: actorSelect },
    })
    const unread = await this.db.activity.count({ where: { organizationId, userId, readAt: null } })
    return { items: rows, unread }
  }

  @Post('read')
  async readAll(@Param('organizationId') organizationId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    await this.db.activity.updateMany({ where: { organizationId, userId, readAt: null }, data: { readAt: new Date() } })
    return { ok: true }
  }

  @Post(':activityId/read')
  async readOne(@Param('organizationId') organizationId: string, @Param('activityId') activityId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    await this.db.activity.updateMany({ where: { id: activityId, userId }, data: { readAt: new Date() } })
    return { ok: true }
  }
}
