import { Body, ConflictException, Controller, Delete, Get, Inject, NotFoundException, Param, Patch, Post, Req, UseGuards } from '@nestjs/common'
import { IsOptional, IsString, Length, Matches } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { cleanPermissions, effectivePermissions, requirePermission } from './permissions'

export class RoleDto {
  @IsString() @Length(2, 24) @Matches(/^[a-z0-9][a-z0-9_-]*$/, { message: 'Rol kimliği yalnızca küçük harf, rakam, tire ve alt çizgi içerebilir.', context: { safe: true } }) handle!: string
  @IsString() @Length(2, 40) name!: string
  @IsOptional() @IsString() @Length(0, 160) description?: string
  @IsOptional() @IsString() @Length(0, 20) color?: string
}
export class RolePatchDto {
  @IsOptional() @IsString() @Length(2, 24) @Matches(/^[a-z0-9][a-z0-9_-]*$/, { message: 'Rol kimliği yalnızca küçük harf, rakam, tire ve alt çizgi içerebilir.', context: { safe: true } }) handle?: string
  @IsOptional() @IsString() @Length(2, 40) name?: string
  @IsOptional() @IsString() @Length(0, 160) description?: string
  @IsOptional() @IsString() @Length(0, 20) color?: string
}

const shape = {
  id: true, handle: true, name: true, description: true, color: true, everyone: true, founder: true, permissions: true, createdAt: true, updatedAt: true,
  members: { orderBy: { addedAt: 'asc' as const }, select: { user: { select: { id: true, username: true, displayName: true, avatarUrl: true, status: true } } } },
  groups: { select: { group: { select: { id: true, handle: true, name: true, color: true, _count: { select: { members: true } } } } } },
}
type Shaped = { members: { user: unknown }[]; groups: { group: { _count: { members: number } } }[] }
const flatten = (role: Shaped) => ({
  ...role,
  members: role.members.map(member => member.user),
  groups: role.groups.map(item => ({ ...item.group, memberCount: item.group._count.members })),
})

// Roller: yetki kümeleri. Hem tek tek kişilere hem de gruplara (@devs) atanır;
// gruba atanan rol, gruptaki herkese geçer. Silinemeyen iki temel rol vardır:
// kurucu (kuruluş sahibi, tüm yetkiler) ve herkes (kuruluştaki herkes).
@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/roles')
export class RolesController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async membership(organizationId: string, userId: string, permission?: 'usergroup.manage' | 'member.roles') {
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
    if (permission) await requirePermission(this.db, organizationId, userId, permission)
    return member
  }

  private async role(organizationId: string, roleId: string) {
    const role = await this.db.role.findFirst({ where: { id: roleId, organizationId }, select: shape })
    if (!role) throw new NotFoundException({ code: 'ROLE_NOT_FOUND', message: 'Rol bulunamadı.' })
    return role
  }

  private guardBase(role: { everyone: boolean; founder: boolean }, action: string) {
    if (role.everyone) throw new ConflictException({ code: 'EVERYONE_PROTECTED', message: `Herkes rolü ${action}.` })
    if (role.founder) throw new ConflictException({ code: 'FOUNDER_PROTECTED', message: `Kurucu rolü ${action}.` })
  }

  // Rolü taşıyan herkesin (doğrudan üyeler + gruplarındaki kişiler) arayüzü tazelenmeli.
  private async notifyHolders(organizationId: string, roleId: string) {
    const role = await this.db.role.findFirst({
      where: { id: roleId },
      select: { everyone: true, members: { select: { userId: true } }, groups: { select: { group: { select: { members: { select: { userId: true } } } } } } },
    })
    if (!role) return
    const ids = role.everyone
      ? (await this.db.organizationMember.findMany({ where: { organizationId }, select: { userId: true } })).map(member => member.userId)
      : [...role.members.map(member => member.userId), ...role.groups.flatMap(item => item.group.members.map(member => member.userId))]
    for (const userId of [...new Set(ids)]) {
      try {
        const effective = await effectivePermissions(this.db, organizationId, userId)
        this.stream.emitToUsers([userId], 'permissions:changed', { organizationId, permissions: effective.permissions })
      } catch {}
    }
  }

  private announce(organizationId: string) { this.stream.emit(organizationId, 'roles:changed', { organizationId }) }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub)
    const rows = await this.db.role.findMany({ where: { organizationId }, orderBy: [{ founder: 'desc' }, { everyone: 'desc' }, { handle: 'asc' }], select: shape })
    return rows.map(flatten)
  }

  @Post()
  async create(@Param('organizationId') organizationId: string, @Req() req: any, @Dto(RoleDto) dto: RoleDto) {
    await this.membership(organizationId, req.user.sub, 'member.roles')
    const handle = dto.handle.trim().toLowerCase()
    if (await this.db.role.findFirst({ where: { organizationId, handle } })) throw new ConflictException({ code: 'HANDLE_TAKEN', message: 'Bu rol kimliği zaten kullanılıyor.', field: 'handle' })
    const role = await this.db.role.create({
      data: { organizationId, handle, name: dto.name.trim(), description: dto.description?.trim() || null, color: dto.color?.trim() || null, createdById: req.user.sub },
      select: shape,
    })
    this.announce(organizationId)
    return flatten(role)
  }

  @Patch(':roleId')
  async update(@Param('organizationId') organizationId: string, @Param('roleId') roleId: string, @Req() req: any, @Dto(RolePatchDto) dto: RolePatchDto) {
    await this.membership(organizationId, req.user.sub, 'member.roles')
    const current = await this.role(organizationId, roleId)
    const data: { handle?: string; name?: string; description?: string | null; color?: string | null } = {}
    if (dto.handle !== undefined && dto.handle.trim().toLowerCase() !== current.handle) {
      this.guardBase(current, 'yeniden adlandırılamaz')
      const handle = dto.handle.trim().toLowerCase()
      if (await this.db.role.findFirst({ where: { organizationId, handle } })) throw new ConflictException({ code: 'HANDLE_TAKEN', message: 'Bu rol kimliği zaten kullanılıyor.', field: 'handle' })
      data.handle = handle
    }
    if (dto.name !== undefined) data.name = dto.name.trim()
    if (dto.description !== undefined) data.description = dto.description.trim() || null
    if (dto.color !== undefined) data.color = dto.color.trim() || null
    const role = await this.db.role.update({ where: { id: roleId }, data, select: shape })
    this.announce(organizationId)
    return flatten(role)
  }

  @Patch(':roleId/permissions')
  async setPermissions(@Param('organizationId') organizationId: string, @Param('roleId') roleId: string, @Req() req: any, @Body() body: { permissions?: unknown }) {
    await this.membership(organizationId, req.user.sub, 'member.roles')
    const current = await this.role(organizationId, roleId)
    if (current.founder) throw new ConflictException({ code: 'FOUNDER_PROTECTED', message: 'Kurucu rolünün yetkileri değiştirilemez.' })
    const role = await this.db.role.update({ where: { id: roleId }, data: { permissions: cleanPermissions(body?.permissions) }, select: shape })
    this.announce(organizationId)
    await this.notifyHolders(organizationId, roleId)
    return flatten(role)
  }

  @Delete(':roleId')
  async remove(@Param('organizationId') organizationId: string, @Param('roleId') roleId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub, 'member.roles')
    this.guardBase(await this.role(organizationId, roleId), 'silinemez')
    await this.notifyHolders(organizationId, roleId)
    await this.db.role.delete({ where: { id: roleId } })
    this.announce(organizationId)
    return { id: roleId, removed: true }
  }

  // Role kişi atama.
  @Post(':roleId/members/:userId')
  async addMember(@Param('organizationId') organizationId: string, @Param('roleId') roleId: string, @Param('userId') userId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub, 'member.roles')
    this.guardBase(await this.role(organizationId, roleId), 'elle düzenlenemez')
    if (!(await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } }))) throw new NotFoundException({ code: 'MEMBER_NOT_FOUND', message: 'Üye bulunamadı.' })
    await this.db.roleMember.createMany({ data: [{ roleId, userId }], skipDuplicates: true })
    this.announce(organizationId)
    await this.notifyHolders(organizationId, roleId)
    return flatten(await this.role(organizationId, roleId))
  }

  @Delete(':roleId/members/:userId')
  async removeMember(@Param('organizationId') organizationId: string, @Param('roleId') roleId: string, @Param('userId') userId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub, 'member.roles')
    this.guardBase(await this.role(organizationId, roleId), 'elle düzenlenemez')
    await this.notifyHolders(organizationId, roleId)
    await this.db.roleMember.deleteMany({ where: { roleId, userId } })
    this.announce(organizationId)
    return flatten(await this.role(organizationId, roleId))
  }

  // Role grup atama: gruptaki herkes rolün yetkilerini kazanır.
  @Post(':roleId/groups/:groupId')
  async addGroup(@Param('organizationId') organizationId: string, @Param('roleId') roleId: string, @Param('groupId') groupId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub, 'member.roles')
    this.guardBase(await this.role(organizationId, roleId), 'gruba bağlanamaz')
    if (!(await this.db.userGroup.findFirst({ where: { id: groupId, organizationId } }))) throw new NotFoundException({ code: 'USER_GROUP_NOT_FOUND', message: 'Grup bulunamadı.' })
    await this.db.roleGroup.createMany({ data: [{ roleId, groupId }], skipDuplicates: true })
    this.announce(organizationId)
    await this.notifyHolders(organizationId, roleId)
    return flatten(await this.role(organizationId, roleId))
  }

  @Delete(':roleId/groups/:groupId')
  async removeGroup(@Param('organizationId') organizationId: string, @Param('roleId') roleId: string, @Param('groupId') groupId: string, @Req() req: any) {
    await this.membership(organizationId, req.user.sub, 'member.roles')
    this.guardBase(await this.role(organizationId, roleId), 'gruba bağlanamaz')
    await this.notifyHolders(organizationId, roleId)
    await this.db.roleGroup.deleteMany({ where: { roleId, groupId } })
    this.announce(organizationId)
    return flatten(await this.role(organizationId, roleId))
  }
}
