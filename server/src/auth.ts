import { BadRequestException, Body, CanActivate, ConflictException, Controller, ExecutionContext, Inject, Injectable, Post, UnauthorizedException } from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { IsBoolean, IsEmail, IsOptional, IsString, Length, Matches, MinLength } from 'class-validator'
import * as argon2 from 'argon2'
import { createHash, randomBytes } from 'node:crypto'
import { PrismaService } from './prisma.service'
import { Dto } from './validation'

export class RegisterDto {
  @IsEmail() email!: string
  @IsString() @Length(3, 24, { message: 'Kullanıcı adı 3-24 karakter olmalı.', context: { safe: true } }) @Matches(/^[a-zA-Z0-9_.]+$/, { message: 'Kullanıcı adı boşluk içeremez; yalnızca harf, rakam, nokta ve alt çizgi kullanılabilir.', context: { safe: true } }) username!: string
  @IsString() @Length(2, 40) displayName!: string
  @IsString() @MinLength(8, { message: 'Parola en az 8 karakter olmalı.', context: { safe: true } }) @Matches(/^(?=.*\p{Ll})(?=.*\p{Lu})(?=.*\d)/u, { message: 'Parola en az bir büyük harf, bir küçük harf ve bir rakam içermeli.', context: { safe: true } }) password!: string
  @IsString() passwordConfirm!: string
}
export class LoginDto { @IsString() identifier!: string; @IsString() password!: string; @IsOptional() @IsBoolean() rememberMe?: boolean }
export class RefreshDto { @IsString() refreshToken!: string }

const safeUser = (u: { id: string; email: string; username: string; displayName: string; status: string }) => ({ id: u.id, email: u.email, username: u.username, displayName: u.displayName, status: u.status })
const digest = (value: string) => createHash('sha256').update(value).digest('hex')

@Injectable()
export class AuthService {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(JwtService) private jwt: JwtService) {}
  private async tokens(user: { id: string; email: string; username: string; displayName: string; status: string }, remember = true) {
    const accessToken = await this.jwt.signAsync({ sub: user.id, username: user.username }, { secret: process.env.JWT_ACCESS_SECRET, expiresIn: '15m' })
    const raw = randomBytes(48).toString('base64url')
    const refreshToken = await this.jwt.signAsync({ sub: user.id, nonce: raw }, { secret: process.env.JWT_REFRESH_SECRET, expiresIn: remember ? '30d' : '1d' })
    const expiresAt = new Date(Date.now() + (remember ? 30 : 1) * 86400000)
    await this.db.session.create({ data: { userId: user.id, tokenHash: digest(refreshToken), expiresAt } })
    return { user: safeUser(user), accessToken, refreshToken }
  }
  async register(dto: RegisterDto, role: 'USER' | 'ADMIN' = 'USER') {
    if (dto.password !== dto.passwordConfirm) throw new BadRequestException({ code: 'PASSWORD_MISMATCH', message: 'Parolalar eşleşmiyor.', field: 'passwordConfirm' })
    const email = dto.email.trim().toLowerCase(), username = dto.username.trim()
    const exists = await this.db.user.findFirst({ where: { OR: [{ email }, { username: { equals: username, mode: 'insensitive' } }] } })
    if (exists) throw new ConflictException({ code: exists.email === email ? 'EMAIL_TAKEN' : 'USERNAME_TAKEN', message: exists.email === email ? 'Bu e-posta zaten kullanılıyor.' : 'Bu kullanıcı adı zaten kullanılıyor.', field: exists.email === email ? 'email' : 'username' })
    const user = await this.db.user.create({ data: { email, username, displayName: dto.displayName.trim(), passwordHash: await argon2.hash(dto.password, { type: argon2.argon2id }), role } })
    return this.tokens(user)
  }
  async login(dto: LoginDto) {
    const key = dto.identifier.trim()
    const user = await this.db.user.findFirst({ where: { OR: [{ email: key.toLowerCase() }, { username: key }] } })
    if (!user || !(await argon2.verify(user.passwordHash, dto.password))) throw new UnauthorizedException({ code: 'INVALID_CREDENTIALS', message: 'E-posta/kullanıcı adı veya parola hatalı.' })
    return this.tokens(user, dto.rememberMe !== false)
  }
  async refresh(token: string) {
    try {
      const payload = await this.jwt.verifyAsync(token, { secret: process.env.JWT_REFRESH_SECRET })
      const session = await this.db.session.findUnique({ where: { tokenHash: digest(token) }, include: { user: true } })
      if (!session || session.userId !== payload.sub || session.expiresAt < new Date()) throw new Error()
      await this.db.session.delete({ where: { id: session.id } })
      return this.tokens(session.user)
    } catch { throw new UnauthorizedException({ code: 'SESSION_EXPIRED', message: 'Oturum süresi doldu.' }) }
  }
  async logout(token?: string) { if (token) await this.db.session.deleteMany({ where: { tokenHash: digest(token) } }); return { success: true } }
}

@Injectable()
export class AuthGuard implements CanActivate {
  constructor(@Inject(JwtService) private jwt: JwtService) {}
  async canActivate(ctx: ExecutionContext) {
    const req = ctx.switchToHttp().getRequest()
    const token = req.headers.authorization?.replace(/^Bearer\s+/i, '')
    if (!token) throw new UnauthorizedException({ code: 'UNAUTHORIZED', message: 'Oturum gerekli.' })
    try { req.user = await this.jwt.verifyAsync(token, { secret: process.env.JWT_ACCESS_SECRET }); return true }
    catch { throw new UnauthorizedException({ code: 'UNAUTHORIZED', message: 'Oturum geçersiz.' }) }
  }
}

@Controller('auth')
export class AuthController {
  constructor(@Inject(AuthService) private auth: AuthService) {}
  @Post('register') register(@Dto(RegisterDto) dto: RegisterDto) { return this.auth.register(dto) }
  @Post('login') login(@Dto(LoginDto) dto: LoginDto) { return this.auth.login(dto) }
  @Post('refresh') refresh(@Dto(RefreshDto) dto: RefreshDto) { return this.auth.refresh(dto.refreshToken) }
  @Post('logout') logout(@Body() dto: Partial<RefreshDto>) { return this.auth.logout(dto.refreshToken) }
}
