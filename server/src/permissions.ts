import { ForbiddenException, NotFoundException } from '@nestjs/common'
import { PrismaService } from './prisma.service'

// Yetki kataloğu. Roller kaba ayrım (sahip/yönetici/üye), yetkiler ince ayar:
// bir yetki kullanıcı gruplarına (@devs), tek tek kişilere ve ayrıca kanal
// bazında ayrı ayrı verilip alınabilir.
export const PERMISSIONS = [
  { id: 'admin.panel', group: 'Yönetim', label: 'Yönetim panelini aç', hint: 'Genel bakış, üyeler ve kanal listesi', channel: false },
  { id: 'organization.manage', group: 'Yönetim', label: 'Kuruluşu düzenle', hint: 'Kuruluş adını değiştir', channel: false },
  { id: 'member.roles', group: 'Yönetim', label: 'Rol ve yetki ata', hint: 'Rolleri, grup ve kişi yetkilerini düzenle', channel: false },
  { id: 'member.invite', group: 'Üyeler', label: 'Davet kodu yönet', hint: 'Yeni davet kodu oluştur ve iptal et', channel: false },
  { id: 'member.kick', group: 'Üyeler', label: 'Üye çıkar', hint: 'Kuruluştan üye çıkar, oturumlarını kapat', channel: false },
  { id: 'usergroup.manage', group: 'Üyeler', label: 'Kullanıcı gruplarını yönet', hint: '@devs gibi grupları oluştur ve düzenle', channel: false },

  { id: 'channel.view', group: 'Kanal', label: 'Kanalı görüntüle', hint: 'Kanalı listede gör ve mesajlarını oku', channel: true },
  { id: 'channel.manage', group: 'Kanal', label: 'Kanalları yönet', hint: 'Kanal ve kategori oluştur, düzenle, sil', channel: true },
  { id: 'channel.permissions', group: 'Kanal', label: 'Kanal izinlerini düzenle', hint: 'Bu kanala özel yetkileri değiştir', channel: true },

  { id: 'message.send', group: 'Mesaj', label: 'Mesaj gönder', hint: 'Kanala yazı yazabilir', channel: true },
  { id: 'message.attach', group: 'Mesaj', label: 'Dosya ekle', hint: 'Görsel, video ve dosya paylaşabilir', channel: true },
  { id: 'message.react', group: 'Mesaj', label: 'Tepki ver', hint: 'Mesajlara emoji tepkisi bırakabilir', channel: true },
  { id: 'message.forward', group: 'Mesaj', label: 'Mesaj ilet', hint: 'Mesajı başka bir kanala iletebilir', channel: true },
  { id: 'message.delete_any', group: 'Mesaj', label: 'Her mesajı sil', hint: 'Başkalarının mesajlarını da silebilir', channel: true },
  { id: 'message.mention_all', group: 'Mesaj', label: '@everyone / @here kullan', hint: 'Herkesi tek seferde etiketleyebilir', channel: true },
  { id: 'thread.reply', group: 'Mesaj', label: 'Thread’e yanıt yaz', hint: 'Konu altında yanıt bırakabilir', channel: true },

  { id: 'call.start', group: 'Sesli', label: 'Arama başlat', hint: 'Kanalda ya da kişiyle arama başlatabilir', channel: true },
  { id: 'voice.connect', group: 'Sesli', label: 'Sesli odaya katıl', hint: 'Sesli kanala bağlanabilir', channel: true },
  { id: 'voice.speak', group: 'Sesli', label: 'Konuş', hint: 'Aramada mikrofonunu açabilir', channel: true },
  { id: 'voice.video', group: 'Sesli', label: 'Kamera aç', hint: 'Aramada kamerasını açabilir', channel: true },
  { id: 'screen.share', group: 'Sesli', label: 'Ekran paylaş', hint: 'Aramada ekranını yayınlayabilir', channel: true },
  { id: 'screen.annotate', group: 'Sesli', label: 'Yayına çizim yap', hint: 'Paylaşılan ekran üzerine kalemle çizebilir', channel: true },

  { id: 'direct.send', group: 'Sohbet', label: 'Bireysel mesaj gönder', hint: 'Kişilere özelden yazabilir', channel: false },
  { id: 'group.create', group: 'Sohbet', label: 'Grup sohbeti oluştur', hint: 'Yeni grup kurup kişi ekleyebilir', channel: false },
] as const

export type PermissionId = typeof PERMISSIONS[number]['id']
export const ALL_PERMISSIONS = PERMISSIONS.map(item => item.id) as PermissionId[]
// Kanal bazında ayrıca ayarlanabilen yetkiler (kanal iznine düşen alt küme).
export const CHANNEL_PERMISSIONS = PERMISSIONS.filter(item => item.channel).map(item => item.id) as PermissionId[]
const known = new Set<string>(ALL_PERMISSIONS)
export const cleanPermissions = (values: unknown) => [...new Set((Array.isArray(values) ? values : []).map(String).filter(item => known.has(item)))] as PermissionId[]
const channelOnly = (values: PermissionId[]) => values.filter(item => (CHANNEL_PERMISSIONS as string[]).includes(item))
export const cleanChannelPermissions = (values: unknown) => channelOnly(cleanPermissions(values))

// Kurucu rolü her zaman tüm yetkileri taşır; düzenlenemez.
export const founderRoleData = (createdById: string) => ({
  handle: 'kurucu', name: 'Kurucu', description: 'Kuruluşu kuran kişi: tüm yetkiler her zaman açıktır.',
  color: '#faa61a', founder: true, permissions: ALL_PERMISSIONS, createdById,
})

// Yeni kuruluşun @everyone rolüne konan başlangıç yetkileri. Bunlar sabit değil;
// kurulduktan sonra rol istendiği gibi düzenlenir.
export const EVERYONE_DEFAULTS: PermissionId[] = [
  'channel.view', 'channel.manage', 'message.send', 'message.attach', 'message.react', 'message.forward', 'thread.reply',
  'call.start', 'voice.connect', 'voice.speak', 'voice.video', 'screen.share', 'screen.annotate', 'direct.send', 'group.create',
]

export type OverrideTarget = 'ROLE' | 'GROUP' | 'USER'
export type Effective = {
  userId: string
  owner: boolean                 // kuruluşu kuran kişi: yetkileri kısıtlanamaz
  permissions: PermissionId[]
  roles: { id: string; handle: string; name: string; color: string | null; everyone: boolean; founder: boolean; permissions: PermissionId[] }[]
  allow: PermissionId[]
  deny: PermissionId[]
  roleIds: string[]
  groupIds: string[]              // etiketlenebilir grup üyelikleri
}

// Kuruluş genelindeki yetki: @everyone rolü + kişiye atanmış roller + kişinin
// üyesi olduğu grupların rolleri + kişiye özel eklenenler − kişiye özel kapatılanlar.
// Kuruluş sahibi her zaman her şeyi yapabilir.
export async function effectivePermissions(db: PrismaService, organizationId: string, userId: string): Promise<Effective> {
  const member = await db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
  if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
  const owner = member.role === 'OWNER'
  const rows = await db.role.findMany({
    where: { organizationId, OR: [{ everyone: true }, { members: { some: { userId } } }, { groups: { some: { group: { members: { some: { userId } } } } } }] },
    orderBy: [{ founder: 'desc' }, { everyone: 'desc' }, { handle: 'asc' }],
    select: { id: true, handle: true, name: true, color: true, everyone: true, founder: true, permissions: true },
  })
  const roles = rows.map(role => ({ ...role, permissions: cleanPermissions(role.permissions) }))
  const allow = cleanPermissions(member.permissions)
  const deny = owner ? [] : cleanPermissions(member.deniedPermissions)
  const granted = owner ? new Set<PermissionId>(ALL_PERMISSIONS) : new Set<PermissionId>([...roles.flatMap(role => role.permissions), ...allow])
  deny.forEach(item => granted.delete(item))
  const groups = await db.userGroupMember.findMany({ where: { userId, group: { organizationId } }, select: { groupId: true } })
  return { userId, owner, permissions: [...granted], roles, allow, deny, roleIds: roles.map(role => role.id), groupIds: groups.map(item => item.groupId) }
}

export type ChannelOverride = { targetType: string; targetId: string; allow: string[]; deny: string[] }

// Kanal izinleri kuruluş genelindeki yetkinin üstüne sırayla uygulanır:
// önce role özel, sonra kullanıcı gruplarına özel, en sonda kişiye özel kural.
// Her katmanda önce kapatmalar, sonra açmalar işlenir; böylece en dar kural
// (kişiye özel) her zaman son sözü söyler.
export function applyChannelOverrides(base: PermissionId[], effective: Effective, overrides: ChannelOverride[]) {
  if (effective.owner) return new Set(base)
  const granted = new Set(base)
  const layer = (rows: ChannelOverride[]) => {
    const deny = new Set(rows.flatMap(row => cleanChannelPermissions(row.deny)))
    const allow = new Set(rows.flatMap(row => cleanChannelPermissions(row.allow)))
    deny.forEach(item => granted.delete(item))
    allow.forEach(item => granted.add(item))
  }
  const everyoneIds = effective.roles.filter(role => role.everyone).map(role => role.id)
  layer(overrides.filter(row => row.targetType === 'ROLE' && everyoneIds.includes(row.targetId)))
  layer(overrides.filter(row => row.targetType === 'ROLE' && !everyoneIds.includes(row.targetId) && effective.roleIds.includes(row.targetId)))
  layer(overrides.filter(row => row.targetType === 'GROUP' && effective.groupIds.includes(row.targetId)))
  layer(overrides.filter(row => row.targetType === 'USER' && row.targetId === effective.userId))
  return granted
}

// Belirli bir kanaldaki etkin yetkiler.
export async function channelPermissions(db: PrismaService, organizationId: string, userId: string, channelId: string) {
  const effective = await effectivePermissions(db, organizationId, userId)
  const rows = await db.channelPermission.findMany({ where: { channelId, OR: [{ targetType: 'ROLE' }, { targetType: 'GROUP' }, { targetType: 'USER', targetId: userId }] } })
  return { effective, permissions: applyChannelOverrides(effective.permissions, effective, rows) }
}

export async function hasPermission(db: PrismaService, organizationId: string, userId: string, permission: PermissionId) {
  return (await effectivePermissions(db, organizationId, userId)).permissions.includes(permission)
}

function refuse(permission: PermissionId): never {
  const label = PERMISSIONS.find(item => item.id === permission)?.label || permission
  throw new ForbiddenException({ code: 'PERMISSION_DENIED', message: `Bu işlem için yetkin yok: ${label}.`, context: { safe: true } })
}

export async function requirePermission(db: PrismaService, organizationId: string, userId: string, permission: PermissionId) {
  const effective = await effectivePermissions(db, organizationId, userId)
  if (!effective.permissions.includes(permission)) refuse(permission)
  return effective
}

export async function requireChannelPermission(db: PrismaService, organizationId: string, userId: string, channelId: string, ...needed: PermissionId[]) {
  const { permissions } = await channelPermissions(db, organizationId, userId, channelId)
  for (const permission of needed) if (!permissions.has(permission)) refuse(permission)
  return permissions
}

// Kişinin görebildiği kanalların kimlikleri; liste ve canlı yayın filtresi bunu kullanır.
export async function visibleChannelIds(db: PrismaService, organizationId: string, userId: string, channelIds: string[]) {
  if (!channelIds.length) return new Set<string>()
  const effective = await effectivePermissions(db, organizationId, userId)
  if (effective.owner) return new Set(channelIds)
  const rows = await db.channelPermission.findMany({ where: { channelId: { in: channelIds }, OR: [{ targetType: 'ROLE' }, { targetType: 'GROUP' }, { targetType: 'USER', targetId: userId }] } })
  const byChannel = new Map<string, ChannelOverride[]>()
  for (const row of rows) byChannel.set(row.channelId, [...(byChannel.get(row.channelId) || []), row])
  const visible = new Set<string>()
  for (const channelId of channelIds) {
    const granted = applyChannelOverrides(effective.permissions, effective, byChannel.get(channelId) || [])
    if (granted.has('channel.view')) visible.add(channelId)
  }
  return visible
}

// Bir kanalı görebilen kişiler. Canlı yayında her mesaj için tek tek sorgu
// atmamak adına üyeler, grup üyelikleri ve kanal kuralları tek seferde okunur.
export async function channelAudience(db: PrismaService, organizationId: string, channelId: string, userIds: string[]) {
  if (!userIds.length) return []
  const [members, roleRows, groupRows, everyoneRoles, overrides] = await Promise.all([
    db.organizationMember.findMany({ where: { organizationId, userId: { in: userIds } }, select: { userId: true, role: true, permissions: true, deniedPermissions: true } }),
    // Kişiye doğrudan atanan roller + üyesi olduğu grupların rolleri.
    db.role.findMany({
      where: { organizationId, everyone: false, OR: [{ members: { some: { userId: { in: userIds } } } }, { groups: { some: { group: { members: { some: { userId: { in: userIds } } } } } } }] },
      select: { id: true, permissions: true, members: { select: { userId: true } }, groups: { select: { group: { select: { members: { select: { userId: true } } } } } } },
    }),
    db.userGroupMember.findMany({ where: { userId: { in: userIds }, group: { organizationId } }, select: { userId: true, groupId: true } }),
    db.role.findMany({ where: { organizationId, everyone: true }, select: { id: true, permissions: true } }),
    db.channelPermission.findMany({ where: { channelId } }),
  ])
  const rolesOf = new Map<string, { id: string; everyone: boolean; permissions: PermissionId[] }[]>()
  for (const role of roleRows) {
    const holders = new Set<string>([...role.members.map(item => item.userId), ...role.groups.flatMap(item => item.group.members.map(member => member.userId))])
    for (const holder of holders) rolesOf.set(holder, [...(rolesOf.get(holder) || []), { id: role.id, everyone: false, permissions: cleanPermissions(role.permissions) }])
  }
  const groupsOf = new Map<string, string[]>()
  for (const row of groupRows) groupsOf.set(row.userId, [...(groupsOf.get(row.userId) || []), row.groupId])
  const base = everyoneRoles.map(role => ({ id: role.id, everyone: true, permissions: cleanPermissions(role.permissions) }))
  return members.filter(member => {
    const owner = member.role === 'OWNER'
    const roles = [...base, ...(rolesOf.get(member.userId) || [])]
    const granted = owner ? new Set<PermissionId>(ALL_PERMISSIONS) : new Set<PermissionId>([...roles.flatMap(role => role.permissions), ...cleanPermissions(member.permissions)])
    if (!owner) cleanPermissions(member.deniedPermissions).forEach(item => granted.delete(item))
    const effective: Effective = {
      userId: member.userId, owner, permissions: [...granted],
      roles: roles.map(role => ({ ...role, handle: '', name: '', color: null, founder: false })),
      allow: [], deny: [], roleIds: roles.map(role => role.id), groupIds: groupsOf.get(member.userId) || [],
    }
    return applyChannelOverrides(effective.permissions, effective, overrides).has('channel.view')
  }).map(member => member.userId)
}
