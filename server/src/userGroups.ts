import { Body, ConflictException, Controller, Delete, Get, Inject, NotFoundException, Param, Patch, Post, Req, UseGuards } from '@nestjs/common'
import { ArrayMaxSize, IsArray, IsOptional, IsString, Length, Matches } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { effectivePermissions, requirePermission } from './permissions'

export class UserGroupDto {
  @IsString() @Length(2, 24) @Matches(/^[a-z0-9][a-z0-9_-]*$/, { message: 'Etiket yalnızca küçük harf, rakam, tire ve alt çizgi içerebilir.', context: { safe: true } }) handle!: string
  @IsString() @Length(2, 40) name!: string
  @IsOptional() @IsString() @Length(0, 160) description?: string
  @IsOptional() @IsString() @Length(0, 20) color?: string
}
export class UserGroupPatchDto {
  @IsOptional() @IsString() @Length(2, 24) @Matches(/^[a-z0-9][a-z0-9_-]*$/, { message: 'Etiket yalnızca küçük harf, rakam, tire ve alt çizgi içerebilir.', context: { safe: true } }) handle?: string
  @IsOptional() @IsString() @Length(2, 40) name?: string
  @IsOptional() @IsString() @Length(0, 160) description?: string
  @IsOptional() @IsString() @Length(0, 20) color?: string
}
export class UserGroupMembersDto {
  @IsArray() @ArrayMaxSize(200) @IsString({ each: true }) userIds!: string[]
}

const shape = {
  id: true, handle: true, name: true, description: true, color: true, createdAt: true, updatedAt: true,
  roles: { select: { role: { select: { id: true, handle: true, name: true, color: true } } } },
  createdBy: { select: { id: true, displayName: true } },
  members: { orderBy: { addedAt: 'asc' as const }, select: { user: { select: { id: true, username: true, displayName: true, avatarUrl: true, status: true } } } },
}
const flatten = (group: { members: { user: unknown }[]; roles: { role: unknown }[] }) => ({ ...group, members: group.members.map(member => member.user), roles: group.roles.map(item => item.role) })

// Gruplar (@devs, @art): etiketlenebilir kişi kümeleri. Kendi yetkileri yoktur —
// yetki rollerden gelir; bir gruba rol atanırsa (Roller bölümü) gruptaki herkes o
// rolün yetkilerini kazanır. Listeyi herkes görebilir (etiket önerisi için gerekli),
// yalnızca "kullanıcı gruplarını yönet" yetkisi olan değiştirebilir.
@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/user-groups')
export class UserGroupsController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async membership(organizationId: string, userId: string, staffOnly = false) {
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
    if (staffOnly) await requirePermission(this.db, organizationId, userId, 'usergroup.manage')
    return member
  }

  private async group(organizationId: string, groupId: string) {
    const group = await this.db.userGroup.findFirst({ where: { id: groupId, organizationId }, select: shape })
    if (!group) throw new NotFoundException({ code: 'USER_GROUP_NOT_FOUND', message: 'Rol bulunamadı.' })
    return group
  }

  private broadcast(organizationId: string, event: string, payload: unknown) {
    this.stream.emit(organizationId, event, payload)
  }

  // Grup üyeliği değişince, gruba bağlı roller yüzünden yetkiler de değişmiş olabilir.
  private async notifyMembers(organizationId: string, userIds: string[]) {
    for (const userId of [...new Set(userIds)]) {
      try {
        const effective = await effectivePermissions(this.db, organizationId, userId)
        this.stream.emitToUsers([userId], 'permissions:changed', { organizationId, permissions: effective.permissions })
      } catch {}
    }
  }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    const rows = await this.db.userGroup.findMany({ where: { organizationId }, orderBy: { handle: 'asc' }, select: shape })
    return rows.map(flatten)
  }

  @Post()
  async create(@Param('organizationId') organizationId: string, @Req() req: any, @Dto(UserGroupDto) dto: UserGroupDto) {
    await this.membership(organizationId, req.user.sub, true)
    const handle = dto.handle.trim().toLowerCase()
    if (await this.db.userGroup.findFirst({ where: { organizationId, handle } })) throw new ConflictException({ code: 'HANDLE_TAKEN', message: 'Bu etiket zaten kullanılıyor.', field: 'handle' })
    if (await this.db.user.findFirst({ where: { username: { equals: handle, mode: 'insensitive' }, organizationMemberships: { some: { organizationId } } } })) throw new ConflictException({ code: 'HANDLE_IS_USER', message: 'Bu etiket bir kullanıcı adıyla çakışıyor.', field: 'handle' })
    const group = await this.db.userGroup.create({
      data: { organizationId, handle, name: dto.name.trim(), description: dto.description?.trim() || null, color: dto.color?.trim() || null, createdById: req.user.sub },
      select: shape,
    })
    this.broadcast(organizationId, 'usergroup:changed', { group: flatten(group) })
    return flatten(group)
  }

  @Patch(':groupId')
  async update(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any, @Dto(UserGroupPatchDto) dto: UserGroupPatchDto) {
    await this.membership(organizationId, req.user.sub, true)
    const current = await this.group(organizationId, groupId)
    const data: { handle?: string; name?: string; description?: string | null; color?: string | null } = {}
    if (dto.handle !== undefined) {
      const handle = dto.handle.trim().toLowerCase()
      if (handle !== current.handle && await this.db.userGroup.findFirst({ where: { organizationId, handle } })) throw new ConflictException({ code: 'HANDLE_TAKEN', message: 'Bu etiket zaten kullanılıyor.', field: 'handle' })
      data.handle = handle
    }
    if (dto.name !== undefined) data.name = dto.name.trim()
    if (dto.description !== undefined) data.description = dto.description.trim() || null
    if (dto.color !== undefined) data.color = dto.color.trim() || null
    const group = await this.db.userGroup.update({ where: { id: groupId }, data, select: shape })
    this.broadcast(organizationId, 'usergroup:changed', { group: flatten(group) })
    return flatten(group)
  }

  @Delete(':groupId')
  async remove(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub, true)
    await this.group(organizationId, groupId)
    await this.db.userGroup.delete({ where: { id: groupId } })
    this.broadcast(organizationId, 'usergroup:removed', { groupId })
    return { id: groupId, removed: true }
  }

  // Yalnızca kuruluş üyeleri gruba eklenebilir.
  @Post(':groupId/members')
  async addMembers(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Req() req: any, @Dto(UserGroupMembersDto) dto: UserGroupMembersDto) {
    await this.membership(organizationId, req.user.sub, true)
    await this.group(organizationId, groupId)
    const allowed = await this.db.organizationMember.findMany({ where: { organizationId, userId: { in: dto.userIds } }, select: { userId: true } })
    if (!allowed.length) throw new ConflictException({ code: 'NO_MEMBERS', message: 'Eklenecek kuruluş üyesi bulunamadı.' })
    await this.db.userGroupMember.createMany({ data: allowed.map(item => ({ groupId, userId: item.userId })), skipDuplicates: true })
    const group = flatten(await this.group(organizationId, groupId))
    this.broadcast(organizationId, 'usergroup:changed', { group })
    await this.notifyMembers(organizationId, allowed.map(item => item.userId))
    return group
  }

  @Delete(':groupId/members/:userId')
  async removeMember(@Param('organizationId') organizationId: string, @Param('groupId') groupId: string, @Param('userId') userId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub, true)
    await this.group(organizationId, groupId)
    await this.db.userGroupMember.deleteMany({ where: { groupId, userId } })
    const group = flatten(await this.group(organizationId, groupId))
    this.broadcast(organizationId, 'usergroup:changed', { group })
    await this.notifyMembers(organizationId, [userId])
    return group
  }
}
