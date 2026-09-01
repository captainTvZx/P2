import { Inject } from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { ConnectedSocket, MessageBody, OnGatewayConnection, OnGatewayDisconnect, SubscribeMessage, WebSocketGateway, WebSocketServer } from '@nestjs/websockets'
import { Server, Socket } from 'socket.io'
import { randomUUID } from 'node:crypto'
import { PrismaService } from './prisma.service'

type CallMedia = { muted: boolean; video: boolean; screen: boolean }
type Call = {
  id: string
  kind: 'direct' | 'channel'
  organizationId: string
  channelId?: string
  createdAt: number
  connectedAt?: number
  caller?: string
  timeout?: ReturnType<typeof setTimeout>
  conversationId?: string
  messageId?: string
  callee?: string
  participants: Map<string, CallMedia>
  invited: Set<string>
}

// Tüm istemciler tek bir "stream" namespace'ine bağlanır: hem çevrimiçi durumu
// hem de mesaj/kanal/proje olayları buradan yayınlanır. Yayın odaları kuruluş
// bazlıdır, çünkü yetkilendirme de kuruluş üyeliği üzerinden yapılıyor.
@WebSocketGateway({ cors: { origin: true }, namespace: 'stream' })
export class RealtimeGateway implements OnGatewayConnection, OnGatewayDisconnect {
  @WebSocketServer() server!: Server
  private online = new Map<string, number>()
  private idle = new Map<string, boolean>()
  private names = new Map<string, string>()
  // Aramalar bellekte tutulur: çağrı geçmişi saklanmıyor, sunucu yeniden başlarsa aramalar düşer.
  private calls = new Map<string, Call>()
  private channelCalls = new Map<string, string>()
  constructor(@Inject(JwtService) private jwt: JwtService, @Inject(PrismaService) private db: PrismaService) {}

  async handleConnection(client: Socket) {
    try {
      const token = String(client.handshake.auth?.token || '')
      const payload = await this.jwt.verifyAsync(token, { secret: process.env.JWT_ACCESS_SECRET })
      const userId = payload.sub as string
      client.data.userId = userId
      client.join(`user:${userId}`)
      this.online.set(userId, (this.online.get(userId) || 0) + 1)
      await this.refresh(userId)
    } catch { client.disconnect(true) }
  }

  async handleDisconnect(client: Socket) {
    const id = client.data.userId as string | undefined
    if (!id) return
    const count = Math.max(0, (this.online.get(id) || 1) - 1)
    if (count) this.online.set(id, count)
    else {
      this.online.delete(id)
      this.idle.delete(id)
      // Son bağlantısı da koptuysa katıldığı tüm aramalardan düşer.
      for (const call of [...this.calls.values()]) if (call.participants.has(id) || call.invited.has(id)) await this.removeFromCall(call, id)
    }
    await this.refresh(id)
  }

  // Kullanıcının seçtiği durum ile bağlantı/boşta bilgisini birleştirip
  // herkesin göreceği durumu üretir. "Görünmez" seçildiyse dışarıya çevrimdışı görünür.
  private async refresh(userId: string) {
    const user = await this.db.user.findUnique({ where: { id: userId }, select: { status: true, statusPreference: true } })
    if (!user) return
    const connected = (this.online.get(userId) || 0) > 0
    const preference = user.statusPreference
    let status: 'ONLINE' | 'AWAY' | 'BUSY' | 'OFFLINE' = 'OFFLINE'
    if (connected) {
      if (preference === 'INVISIBLE') status = 'OFFLINE'
      else if (preference === 'BUSY') status = 'BUSY'
      else if (preference === 'AWAY') status = 'AWAY'
      else status = this.idle.get(userId) ? 'AWAY' : 'ONLINE'
    }
    if (status !== user.status) await this.db.user.update({ where: { id: userId }, data: { status } })
    this.server?.emit('presence:update', { userId, status })
    this.server?.to(`user:${userId}`).emit('presence:self', { preference, status })
  }

  // "yazıyor" bildirimi: kanalda kuruluş odasına, bireysel sohbette iki katılımcıya.
  @SubscribeMessage('typing')
  async typing(@ConnectedSocket() client: Socket, @MessageBody() body: { organizationId?: string; channelId?: string; conversationId?: string; typing?: boolean }) {
    const userId = client.data.userId as string | undefined
    if (!userId) return { ok: false }
    let displayName = this.names.get(userId)
    if (!displayName) {
      const user = await this.db.user.findUnique({ where: { id: userId }, select: { displayName: true } })
      if (!user) return { ok: false }
      displayName = user.displayName
      this.names.set(userId, displayName)
    }
    const payload = { userId, displayName, channelId: body?.channelId || null, conversationId: body?.conversationId || null, typing: body?.typing !== false }
    if (body?.conversationId) {
      const conversation = await this.db.directConversation.findFirst({ where: { id: body.conversationId, OR: [{ userAId: userId }, { userBId: userId }] }, select: { userAId: true, userBId: true } })
      if (!conversation) return { ok: false }
      this.emitToUsers([conversation.userAId, conversation.userBId], 'typing', payload)
      return { ok: true }
    }
    if (body?.organizationId && body?.channelId) {
      if (!(await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId: body.organizationId, userId } } }))) return { ok: false }
      this.server?.to(`org:${body.organizationId}`).emit('typing', payload)
      return { ok: true }
    }
    return { ok: false }
  }

  @SubscribeMessage('presence:prefer')
  async prefer(@ConnectedSocket() client: Socket, @MessageBody() body: { status?: string }) {
    const userId = client.data.userId as string | undefined
    const preference = String(body?.status || '')
    if (!userId || !['ONLINE', 'AWAY', 'BUSY', 'INVISIBLE'].includes(preference)) return { ok: false }
    await this.db.user.update({ where: { id: userId }, data: { statusPreference: preference } })
    await this.refresh(userId)
    return { ok: true, preference }
  }

  // Uygulama arka plana atıldığında/boşta kaldığında otomatik "uzakta".
  @SubscribeMessage('presence:idle')
  async setIdle(@ConnectedSocket() client: Socket, @MessageBody() body: { away?: boolean }) {
    const userId = client.data.userId as string | undefined
    if (!userId) return { ok: false }
    if (body?.away) this.idle.set(userId, true); else this.idle.delete(userId)
    await this.refresh(userId)
    return { ok: true }
  }

  // ---------------- Sesli/görüntülü arama ----------------

  private async displayNameOf(userId: string) {
    const cached = this.names.get(userId)
    if (cached) return cached
    const user = await this.db.user.findUnique({ where: { id: userId }, select: { displayName: true } })
    if (!user) return ''
    this.names.set(userId, user.displayName)
    return user.displayName
  }

  private async isMember(organizationId: string, userId: string) {
    return Boolean(await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } }))
  }

  private async callPolicy(userId: string): Promise<'ring' | 'silent' | 'reject'> {
    if (!(this.online.get(userId) || 0)) return 'reject'
    const user = await this.db.user.findUnique({ where: { id: userId }, select: { statusPreference: true } })
    if (!user || user.statusPreference === 'INVISIBLE') return 'reject'
    return user.statusPreference === 'BUSY' ? 'silent' : 'ring'
  }

  private async publicCall(call: Call) {
    const participants = await Promise.all([...call.participants.entries()].map(async ([userId, media]) => ({ userId, displayName: await this.displayNameOf(userId), ...media })))
    return {
      id: call.id,
      kind: call.kind,
      organizationId: call.organizationId,
      channelId: call.channelId ?? null,
      caller: call.caller ?? null,
      callee: call.callee ?? null,
      participants,
      invited: [...call.invited],
    }
  }

  private async broadcastCall(call: Call) {
    const payload = await this.publicCall(call)
    this.emitToUsers([...call.participants.keys(), ...call.invited], 'call:state', { call: payload })
    if (call.channelId) this.emit(call.organizationId, 'voice:presence', { channelId: call.channelId, callId: call.id, participants: payload.participants })
  }

  private async endCall(call: Call, reason = 'ended', audience?: string[]) {
    if (call.timeout) { clearTimeout(call.timeout); call.timeout = undefined }
    this.calls.delete(call.id)
    await this.finishCallLog(call)
    if (call.channelId && this.channelCalls.get(call.channelId) === call.id) {
      this.channelCalls.delete(call.channelId)
      this.emit(call.organizationId, 'voice:presence', { channelId: call.channelId, callId: call.id, participants: [] })
    }
    this.emitToUsers(audience ?? [...call.participants.keys(), ...call.invited], 'call:ended', { callId: call.id, reason })
  }

  // Kullanıcıyı çağrıdan düşürür; kimse kalmazsa çağrı kapanır.
  private async removeFromCall(call: Call, userId: string) {
    const audience = [...call.participants.keys(), ...call.invited]
    call.participants.delete(userId)
    call.invited.delete(userId)
    // Birebir aramada tek kişi kalırsa arama biter; sesli oda tek kişiyle de sürer.
    if (!call.participants.size || (call.kind === 'direct' && call.participants.size < 2 && !call.invited.size)) { await this.endCall(call, 'ended', audience); return }
    this.emitToUsers([...call.participants.keys(), ...call.invited], 'call:left', { callId: call.id, userId })
    await this.broadcastCall(call)
  }

  // Birebir aramalar sohbete bir satır bırakır: "arıyor" -> "cevapsız" / "süre".
  private async openConversation(organizationId: string, userA: string, userB: string) {
    const [userAId, userBId] = [userA, userB].sort()
    const existing = await this.db.directConversation.findUnique({ where: { organizationId_userAId_userBId: { organizationId, userAId, userBId } }, select: { id: true } })
    if (existing) return existing.id
    const created = await this.db.directConversation.create({ data: { organizationId, userAId, userBId }, select: { id: true } })
    return created.id
  }

  private async writeCallMessage(call: Call, kind: string, content: string) {
    if (call.kind !== 'direct') return
    const caller = call.caller
    const other = call.callee
    if (!caller || !other) return
    try {
      if (!call.conversationId) call.conversationId = await this.openConversation(call.organizationId, caller, other)
      const select = { id: true, content: true, kind: true, createdAt: true, editedAt: true, author: { select: { id: true, username: true, displayName: true } }, quotedMessage: { select: { id: true, content: true, author: { select: { displayName: true } } } } }
      if (!call.messageId) {
        const message = await this.db.directMessage.create({ data: { conversationId: call.conversationId, authorId: caller, content, kind }, select })
        call.messageId = message.id
        await this.db.directConversation.update({ where: { id: call.conversationId }, data: { updatedAt: new Date() } })
        this.emitToUsers([caller, other], 'direct:created', { conversationId: call.conversationId, message })
        return
      }
      const message = await this.db.directMessage.update({ where: { id: call.messageId }, data: { content, kind }, select })
      this.emitToUsers([caller, other], 'direct:updated', { conversationId: call.conversationId, message })
    } catch { /* sohbet kaydı arama akışını bozmamalı */ }
  }

  private async finishCallLog(call: Call) {
    if (call.kind !== 'direct' || !call.messageId) return
    if (call.connectedAt) {
      const seconds = Math.max(1, Math.round((Date.now() - call.connectedAt) / 1000))
      await this.writeCallMessage(call, 'CALL_ENDED', String(seconds))
    } else {
      await this.writeCallMessage(call, 'CALL_MISSED', '0')
    }
  }

  @SubscribeMessage('call:start')
  async callStart(@ConnectedSocket() client: Socket, @MessageBody() body: { organizationId?: string; kind?: 'direct' | 'channel'; targetUserId?: string; channelId?: string; ring?: boolean }) {
    const userId = client.data.userId as string | undefined
    const organizationId = String(body?.organizationId || '')
    if (!userId || !organizationId || !(await this.isMember(organizationId, userId))) return { ok: false }

    if (body?.kind === 'channel') {
      const channelId = String(body.channelId || '')
      const channel = await this.db.projectChannel.findFirst({ where: { id: channelId, deletedAt: null, category: { project: { organizationId } } }, select: { id: true } })
      if (!channel) return { ok: false }
      // Kanalda zaten bir arama varsa yenisini açmak yerine ona katılınır.
      const existingId = this.channelCalls.get(channelId)
      const existing = existingId ? this.calls.get(existingId) : undefined
      const call: Call = existing ?? { id: randomUUID(), kind: 'channel', organizationId, channelId, createdAt: Date.now(), participants: new Map(), invited: new Set() }
      call.participants.set(userId, call.participants.get(userId) ?? { muted: false, video: false, screen: false })
      call.invited.delete(userId)
      this.calls.set(call.id, call)
      this.channelCalls.set(channelId, call.id)
      // "Kanalda arama başlat": kuruluştaki herkesin telefonu çalar. Sesli odaya
      // sessizce katılırken (ring=false) kimse rahatsız edilmez.
      if (body.ring) {
        const members = await this.db.organizationMember.findMany({ where: { organizationId }, select: { userId: true } })
        const fresh = members.map(member => member.userId).filter(id => id !== userId && !call.participants.has(id) && !call.invited.has(id))
        const policies = await Promise.all(fresh.map(async id => ({ id, policy: await this.callPolicy(id) })))
        const reachable = policies.filter(item => item.policy !== 'reject')
        reachable.forEach(item => call.invited.add(item.id))
        if (reachable.length) {
          const payload = await this.publicCall(call), from = { userId, displayName: await this.displayNameOf(userId) }
          reachable.forEach(item => this.emitToUsers([item.id], 'call:incoming', { call: payload, from, silent: item.policy === 'silent' }))
        }
      }
      await this.broadcastCall(call)
      return { ok: true, call: await this.publicCall(call) }
    }

    const targetUserId = String(body?.targetUserId || '')
    if (!targetUserId || targetUserId === userId || !(await this.isMember(organizationId, targetUserId))) return { ok: false }
    // Karşı taraf bizi hâlâ arıyorsa yeni ve çakışan bir çağrı oluşturma. Geri arama,
    // mevcut daveti kabul etmiş sayılır ve iki istemci aynı call id üzerinde buluşur.
    const existing = [...this.calls.values()].find(item => item.kind === 'direct' && item.organizationId === organizationId &&
      ((item.caller === userId && item.callee === targetUserId) || (item.caller === targetUserId && item.callee === userId)))
    if (existing) {
      if (existing.invited.has(userId) && existing.participants.has(targetUserId)) {
        existing.invited.delete(userId)
        existing.participants.set(userId, { muted: false, video: false, screen: false })
        if (!existing.connectedAt) existing.connectedAt = Date.now()
        if (existing.timeout) { clearTimeout(existing.timeout); existing.timeout = undefined }
        await this.broadcastCall(existing)
      }
      return { ok: true, call: await this.publicCall(existing) }
    }
    const policy = await this.callPolicy(targetUserId)
    const call: Call = { id: randomUUID(), kind: 'direct', organizationId, createdAt: Date.now(), participants: new Map([[userId, { muted: false, video: false, screen: false }]]), invited: new Set([targetUserId]) }
    call.caller = userId
    call.callee = targetUserId
    // 45 saniye içinde kimse açmazsa (kişi çevrimdışıysa da) arama cevapsız olarak kapanır.
    call.timeout = setTimeout(() => { const live = this.calls.get(call.id); if (live && !live.connectedAt) void this.endCall(live, 'missed', [userId, targetUserId]) }, 45000)
    this.calls.set(call.id, call)
    const payload = await this.publicCall(call)
    if (policy !== 'reject') this.emitToUsers([targetUserId], 'call:incoming', { call: payload, from: { userId, displayName: await this.displayNameOf(userId) }, silent: policy === 'silent' })
    await this.writeCallMessage(call, 'CALL_STARTED', '0')
    await this.broadcastCall(call)
    return { ok: true, call: payload }
  }

  @SubscribeMessage('call:invite')
  async callInvite(@ConnectedSocket() client: Socket, @MessageBody() body: { callId?: string; userId?: string }) {
    const userId = client.data.userId as string | undefined
    const call = this.calls.get(String(body?.callId || ''))
    const targetUserId = String(body?.userId || '')
    if (!userId || !call || !call.participants.has(userId)) return { ok: false }
    if (!targetUserId || call.participants.has(targetUserId) || !(await this.isMember(call.organizationId, targetUserId))) return { ok: false }
    const policy = await this.callPolicy(targetUserId)
    if (policy === 'reject') return { ok: false, reason: 'unavailable' }
    call.invited.add(targetUserId)
    this.emitToUsers([targetUserId], 'call:incoming', { call: await this.publicCall(call), from: { userId, displayName: await this.displayNameOf(userId) }, silent: policy === 'silent' })
    await this.broadcastCall(call)
    return { ok: true }
  }

  @SubscribeMessage('call:accept')
  async callAccept(@ConnectedSocket() client: Socket, @MessageBody() body: { callId?: string }) {
    const userId = client.data.userId as string | undefined
    const call = this.calls.get(String(body?.callId || ''))
    if (!userId || !call) return { ok: false }
    // Kanal aramalarına davet beklemeden katılınabilir; birebir aramada davetli olmak gerekir.
    if (call.kind === 'direct' && !call.invited.has(userId) && !call.participants.has(userId)) return { ok: false }
    if (!(await this.isMember(call.organizationId, userId))) return { ok: false }
    call.invited.delete(userId)
    call.participants.set(userId, call.participants.get(userId) ?? { muted: false, video: false, screen: false })
    if (!call.connectedAt && call.participants.size > 1) { call.connectedAt = Date.now(); if (call.timeout) { clearTimeout(call.timeout); call.timeout = undefined } }
    await this.broadcastCall(call)
    return { ok: true, call: await this.publicCall(call) }
  }

  @SubscribeMessage('call:decline')
  async callDecline(@ConnectedSocket() client: Socket, @MessageBody() body: { callId?: string }) {
    const userId = client.data.userId as string | undefined
    const call = this.calls.get(String(body?.callId || ''))
    if (!userId || !call) return { ok: false }
    call.invited.delete(userId)
    this.emitToUsers([...call.participants.keys()], 'call:declined', { callId: call.id, userId })
    // Birebir aramada karşı taraf reddederse çağrı biter.
    if (call.kind === 'direct' && !call.invited.size && call.participants.size < 2) await this.endCall(call, 'declined')
    else await this.broadcastCall(call)
    return { ok: true }
  }

  @SubscribeMessage('call:leave')
  async callLeave(@ConnectedSocket() client: Socket, @MessageBody() body: { callId?: string }) {
    const userId = client.data.userId as string | undefined
    const call = this.calls.get(String(body?.callId || ''))
    if (!userId || !call) return { ok: false }
    await this.removeFromCall(call, userId)
    return { ok: true }
  }

  @SubscribeMessage('call:media')
  async callMedia(@ConnectedSocket() client: Socket, @MessageBody() body: { callId?: string; muted?: boolean; video?: boolean; screen?: boolean }) {
    const userId = client.data.userId as string | undefined
    const call = this.calls.get(String(body?.callId || ''))
    const media = userId ? call?.participants.get(userId) : undefined
    if (!userId || !call || !media) return { ok: false }
    call.participants.set(userId, {
      muted: body?.muted ?? media.muted,
      video: body?.video ?? media.video,
      screen: body?.screen ?? media.screen,
    })
    await this.broadcastCall(call)
    return { ok: true }
  }

  // SDP/ICE yönlendirmesi: yalnızca aynı çağrının katılımcıları arasında.
  @SubscribeMessage('call:signal')
  async callSignal(@ConnectedSocket() client: Socket, @MessageBody() body: { callId?: string; to?: string; data?: unknown }) {
    const userId = client.data.userId as string | undefined
    const call = this.calls.get(String(body?.callId || ''))
    const to = String(body?.to || '')
    if (!userId || !call || !call.participants.has(userId) || !call.participants.has(to)) return { ok: false }
    this.emitToUsers([to], 'call:signal', { callId: call.id, from: userId, data: body?.data })
    return { ok: true }
  }

  @SubscribeMessage('subscribe')
  async subscribe(@ConnectedSocket() client: Socket, @MessageBody() body: { organizationId?: string }) {
    const userId = client.data.userId as string | undefined
    const organizationId = String(body?.organizationId || '')
    if (!userId || !organizationId) return { ok: false }
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) return { ok: false }
    client.join(`org:${organizationId}`)
    return { ok: true }
  }

  // Bireysel sohbetler kurulus odasina degil, yalnizca iki katilimciya gider.
  emitToUsers(userIds: string[], event: string, payload: unknown) {
    for (const id of [...new Set(userIds)]) this.server?.to('user:' + id).emit(event, payload)
  }

  emit(organizationId: string, event: string, payload: unknown) {
    this.server?.to(`org:${organizationId}`).emit(event, payload)
  }
}
