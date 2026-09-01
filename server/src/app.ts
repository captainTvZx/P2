import { Body, ConflictException, Controller, Get, Inject, Module, Post, Req, UseGuards } from '@nestjs/common'
import { JwtModule } from '@nestjs/jwt'
import { AuthController, AuthGuard, AuthService, RegisterDto } from './auth'
import { PrismaService } from './prisma.service'
import { OrganizationsController, OrganizationsService } from './organizations'
import { DirectController } from './direct'
import { PreferencesController } from './preferences'
import { ProjectsController } from './projects'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'

@Controller()
export class AppController {
  constructor(@Inject(PrismaService) private db: PrismaService) {}
  @Get('health') health() { return { status: 'ok' } }
  @UseGuards(AuthGuard) @Get('auth/me') async me(@Req() req: any) { const u = await this.db.user.findUniqueOrThrow({ where: { id: req.user.sub } }); return { id: u.id, email: u.email, username: u.username, displayName: u.displayName, status: u.status } }
  @UseGuards(AuthGuard) @Get('projects') projects(@Req() req: any) { return this.db.project.findMany({ where: { OR: [{ ownerId: req.user.sub }, { members: { some: { userId: req.user.sub } } }] }, orderBy: { updatedAt: 'desc' } }) }
  @UseGuards(AuthGuard) @Get('friends') async friends(@Req() req: any) {
    const rows = await this.db.friendship.findMany({ where: { status: 'ACCEPTED', OR: [{ requesterId: req.user.sub }, { addresseeId: req.user.sub }] }, include: { requester: true, addressee: true } })
    return rows.map(r => { const u = r.requesterId === req.user.sub ? r.addressee : r.requester; return { id: u.id, username: u.username, displayName: u.displayName, status: u.status } })
  }
}

@Controller('setup')
export class SetupController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(AuthService) private auth: AuthService) {}
  @Get('status') async status() { return { installed: (await this.db.user.count({ where: { role: 'ADMIN' } })) > 0 } }
  @Post('admin') async admin(@Dto(RegisterDto) dto: RegisterDto) {
    if (await this.db.user.count({ where: { role: 'ADMIN' } })) throw new ConflictException({ code: 'ALREADY_INSTALLED', message: 'P2 kurulumu zaten tamamlanmış.' })
    return this.auth.register(dto, 'ADMIN')
  }
}

@Module({ imports: [JwtModule.register({})], controllers: [AppController, AuthController, SetupController, OrganizationsController, ProjectsController, DirectController, PreferencesController], providers: [PrismaService, AuthService, AuthGuard, RealtimeGateway, OrganizationsService] })
export class AppModule {}
