import { BadRequestException, Body, ConflictException, Controller, Get, Inject, Injectable, NotFoundException, Param, Post, Req, UseGuards } from '@nestjs/common'
import { IsString, Length, Matches } from 'class-validator'
import { randomBytes } from 'node:crypto'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { Dto } from './validation'

export class CreateOrganizationDto {
  @IsString() @Length(2, 60) name!: string
  @IsString() @Length(2, 40) @Matches(/^[a-z0-9-]+$/) slug!: string
}
export class JoinOrganizationDto { @IsString() @Length(6, 20) code!: string }

@Injectable()
export class OrganizationsService {
  constructor(@Inject(PrismaService) private db: PrismaService) {}
  list(userId: string) {
    return this.db.organizationMember.findMany({
      where: { userId }, orderBy: { joinedAt: 'asc' },
      select: { role: true, joinedAt: true, organization: { select: { id: true, name: true, slug: true, invites: { take: 1, orderBy: { createdAt: 'desc' }, select: { code: true } } } } },
    }).then(rows => rows.map(row => ({ id: row.organization.id, name: row.organization.name, slug: row.organization.slug, role: row.role, inviteCode: row.role === 'OWNER' || row.role === 'ADMIN' ? row.organization.invites[0]?.code : undefined })))
  }
  async create(userId: string, dto: CreateOrganizationDto) {
    const name = dto.name.trim(), slug = dto.slug.trim().toLowerCase()
    if (await this.db.organization.findUnique({ where: { slug } })) throw new ConflictException({ code: 'SLUG_TAKEN', message: 'Bu kuruluş adresi zaten kullanılıyor.', field: 'slug' })
    const code = `P2-${randomBytes(4).toString('hex').toUpperCase()}`
    return this.db.$transaction(async tx => {
      const organization = await tx.organization.create({ data: { name, slug, members: { create: { userId, role: 'OWNER' } }, invites: { create: { code, createdById: userId } } } })
      return { ...organization, role: 'OWNER' as const, inviteCode: code }
    })
  }
  async join(userId: string, rawCode: string) {
    const code = rawCode.trim().toUpperCase().replace(/\s+/g, '')
    const invite = await this.db.organizationInvite.findUnique({ where: { code }, include: { organization: true } })
    if (!invite) throw new NotFoundException({ code: 'INVITE_NOT_FOUND', message: 'Davet kodu bulunamadı.' })
    if (invite.expiresAt && invite.expiresAt < new Date()) throw new BadRequestException({ code: 'INVITE_EXPIRED', message: 'Bu davetin süresi dolmuş.' })
    if (invite.maxUses !== null && invite.useCount >= invite.maxUses) throw new BadRequestException({ code: 'INVITE_FULL', message: 'Bu davetin kullanım sınırı dolmuş.' })
    const existing = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId: invite.organizationId, userId } } })
    if (existing) return { ...invite.organization, role: existing.role }
    await this.db.$transaction([
      this.db.organizationMember.create({ data: { organizationId: invite.organizationId, userId, role: 'MEMBER' } }),
      this.db.organizationInvite.update({ where: { id: invite.id }, data: { useCount: { increment: 1 } } }),
    ])
    return { ...invite.organization, role: 'MEMBER' as const }
  }
  async members(organizationId:string,userId:string){if(!(await this.db.organizationMember.findUnique({where:{organizationId_userId:{organizationId,userId}}})))throw new NotFoundException({code:'ORGANIZATION_NOT_FOUND',message:'Kuruluş bulunamadı.'});return this.db.organizationMember.findMany({where:{organizationId},orderBy:{joinedAt:'asc'},select:{role:true,user:{select:{id:true,username:true,displayName:true,status:true}}}}).then(rows=>rows.map(row=>({...row.user,role:row.role})))}
}

@UseGuards(AuthGuard)
@Controller('organizations')
export class OrganizationsController {
  constructor(@Inject(OrganizationsService) private organizations: OrganizationsService) {}
  @Get() list(@Req() req: any) { return this.organizations.list(req.user.sub) }
  @Post() create(@Req() req: any, @Dto(CreateOrganizationDto) dto: CreateOrganizationDto) { return this.organizations.create(req.user.sub, dto) }
  @Post('join') join(@Req() req: any, @Dto(JoinOrganizationDto) dto: JoinOrganizationDto) { return this.organizations.join(req.user.sub, dto.code) }
  @Get(':organizationId/members') members(@Req() req:any,@Param('organizationId') organizationId:string){return this.organizations.members(organizationId,req.user.sub)}
}
