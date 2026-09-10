import { Controller, Get, Inject, NotFoundException, Param, Put, Req, UseGuards } from '@nestjs/common'
import { IsBoolean, IsIn as IsOneOf, IsOptional, IsString } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { Dto } from './validation'

export class PreferenceDto {
  @IsString() @IsOneOf(['CHANNEL', 'PERSON']) kind!: 'CHANNEL' | 'PERSON'
  @IsString() targetId!: string
  @IsOptional() @IsBoolean() starred?: boolean
  @IsOptional() @IsString() @IsOneOf(['ALL', 'MENTIONS', 'MUTED']) notify?: 'ALL' | 'MENTIONS' | 'MUTED'
  @IsOptional() @IsBoolean() hidden?: boolean   // kanaldan ayrıldıysa kenar çubuğunda gizlenir
}

// Kanal ve kişi başına yıldız / bildirim tercihleri.
@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/preferences')
export class PreferencesController {
  constructor(@Inject(PrismaService) private db: PrismaService) {}

  private async membership(organizationId: string, userId: string) {
    const member = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!member) throw new NotFoundException({ code: 'ORGANIZATION_NOT_FOUND', message: 'Kuruluş bulunamadı.' })
    return member
  }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Req() req: any) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    return this.db.userPreference.findMany({ where: { userId, organizationId }, select: { kind: true, targetId: true, starred: true, notify: true, hidden: true } })
  }

  @Put()
  async save(@Param('organizationId') organizationId: string, @Req() req: any, @Dto(PreferenceDto) dto: PreferenceDto) {
    const userId = req.user.sub as string
    await this.membership(organizationId, userId)
    const data = { ...(dto.starred === undefined ? {} : { starred: dto.starred }), ...(dto.notify === undefined ? {} : { notify: dto.notify }), ...(dto.hidden === undefined ? {} : { hidden: dto.hidden }) }
    return this.db.userPreference.upsert({
      where: { userId_kind_targetId: { userId, kind: dto.kind, targetId: dto.targetId } },
      create: { userId, organizationId, kind: dto.kind, targetId: dto.targetId, starred: dto.starred ?? false, notify: dto.notify ?? 'MENTIONS', hidden: dto.hidden ?? false },
      update: data,
      select: { kind: true, targetId: true, starred: true, notify: true, hidden: true },
    })
  }
}
