import { ConflictException, Controller, Delete, Get, Inject, NotFoundException, Param, Patch, Post, Query, Req, UseGuards } from '@nestjs/common'
import { IsBoolean, IsIn, IsInt, IsOptional, IsString, Length, Min } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { effectivePermissions, requireProjectAccess } from './permissions'
import { pageArgs } from './paging'
import { canEditNote, nextVersion, normalizeVisibility, noteVisibleTo, versionConflicts } from './notesRules'

// Proje notları: oyun içi overlay "Notlar" penceresi ve Kütüphane "Notlar" sekmesi.
// PRIVATE not yalnız yazarına döner/yayınlanır; TEAM not proje erişimi olan herkese.
// PATCH iyimser kilitle çalışır: istemci bildiği `version`ı gönderir, farklıysa 409 NOTE_CONFLICT
// (gövdede güncel not vardır; istemci yeniden yükler).

export class CreateNoteDto {
  @IsOptional() @IsString() @Length(0, 200) title?: string
  @IsOptional() @IsString() @Length(0, 20000) body?: string
  @IsOptional() @IsIn(['PRIVATE', 'TEAM']) visibility?: 'PRIVATE' | 'TEAM'
  @IsOptional() @IsBoolean() pinned?: boolean
}
export class UpdateNoteDto {
  @IsOptional() @IsString() @Length(0, 200) title?: string
  @IsOptional() @IsString() @Length(0, 20000) body?: string
  @IsOptional() @IsIn(['PRIVATE', 'TEAM']) visibility?: 'PRIVATE' | 'TEAM'
  @IsOptional() @IsBoolean() pinned?: boolean
  @IsOptional() @IsInt() @Min(1) version?: number
}

const authorSelect = { select: { id: true, username: true, displayName: true, avatarUrl: true } }
type NoteRow = { id: string; projectId: string; authorId: string; visibility: string }

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects/:projectId/notes')
export class NotesController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  private async access(organizationId: string, projectId: string, userId: string) {
    await requireProjectAccess(this.db, organizationId, userId, projectId)
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId, deletedAt: null }, select: { id: true } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    const effective = await effectivePermissions(this.db, organizationId, userId)
    // Hub ile aynı vekil: projeyi yöneten (channel.manage) ekip notlarını da düzenler/siler.
    return { canManage: effective.permissions.includes('channel.manage' as any) }
  }

  private async find(projectId: string, noteId: string, userId: string) {
    const note = await this.db.projectNote.findFirst({ where: { id: noteId, projectId }, include: { author: authorSelect } })
    if (!note || !noteVisibleTo(note, userId)) throw new NotFoundException({ code: 'NOTE_NOT_FOUND', message: 'Not bulunamadı.' })
    return note
  }

  // PRIVATE → yalnız yazar; TEAM → proje izleyicileri. Görünürlük değişince eski kitle de haberdar
  // edilir (TEAM→PRIVATE: ekip için "silindi", yazar için "güncellendi").
  private changed(organizationId: string, note: NoteRow, action: 'created' | 'updated' | 'deleted', payloadNote?: unknown, previousVisibility?: string) {
    const base = { projectId: note.projectId, action, noteId: note.id, note: payloadNote }
    if (note.visibility === 'TEAM') { void this.stream.emitProject(organizationId, note.projectId, 'note:changed', base); return }
    this.stream.emitToUsers([note.authorId], 'note:changed', base)
    if (previousVisibility === 'TEAM' && action !== 'deleted') void this.stream.emitProject(organizationId, note.projectId, 'note:changed', { projectId: note.projectId, action: 'deleted', noteId: note.id, hiddenFor: note.authorId })
  }

  @Get()
  async list(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Query('before') before?: string, @Query('limit') limit?: string) {
    const userId = req.user.sub as string
    await this.access(organizationId, projectId, userId)
    const page = pageArgs(before, limit, { take: 200, max: 500 })
    const cursor = 'createdAt' in page.where ? { updatedAt: page.where.createdAt } : {}
    const notes = await this.db.projectNote.findMany({
      where: { projectId, OR: [{ visibility: 'TEAM' }, { authorId: userId }], ...cursor },
      orderBy: [{ pinned: 'desc' }, { updatedAt: 'desc' }],
      take: page.take,
      include: { author: authorSelect },
    })
    return notes
  }

  @Post()
  async create(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any, @Dto(CreateNoteDto) dto: CreateNoteDto) {
    const userId = req.user.sub as string
    await this.access(organizationId, projectId, userId)
    const note = await this.db.projectNote.create({
      data: { projectId, authorId: userId, title: (dto.title || '').slice(0, 200), body: dto.body || '', visibility: normalizeVisibility(dto.visibility), pinned: Boolean(dto.pinned) },
      include: { author: authorSelect },
    })
    this.changed(organizationId, note, 'created', note)
    return note
  }

  @Patch(':noteId')
  async update(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('noteId') noteId: string, @Req() req: any, @Dto(UpdateNoteDto) dto: UpdateNoteDto) {
    const userId = req.user.sub as string
    const { canManage } = await this.access(organizationId, projectId, userId)
    const current = await this.find(projectId, noteId, userId)
    if (!canEditNote(current, userId, canManage)) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Bu notu düzenleyemezsin.' })
    if (versionConflicts(current.version, dto.version)) throw new ConflictException({ code: 'NOTE_CONFLICT', message: 'Not başka bir cihazda değişti.', note: current })
    const data: Record<string, unknown> = { version: nextVersion(current.version) }
    if (dto.title !== undefined) data.title = dto.title.slice(0, 200)
    if (dto.body !== undefined) data.body = dto.body
    if (dto.visibility !== undefined) data.visibility = normalizeVisibility(dto.visibility, current.visibility as 'PRIVATE' | 'TEAM')
    if (dto.pinned !== undefined) data.pinned = Boolean(dto.pinned)
    const note = await this.db.projectNote.update({ where: { id: noteId }, data, include: { author: authorSelect } })
    this.changed(organizationId, note, 'updated', note, current.visibility)
    return note
  }

  @Delete(':noteId')
  async remove(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('noteId') noteId: string, @Req() req: any) {
    const userId = req.user.sub as string
    const { canManage } = await this.access(organizationId, projectId, userId)
    const current = await this.find(projectId, noteId, userId)
    if (!canEditNote(current, userId, canManage)) throw new NotFoundException({ code: 'PERMISSION_DENIED', message: 'Bu notu silemezsin.' })
    await this.db.projectNote.delete({ where: { id: noteId } })
    this.changed(organizationId, current, 'deleted')
    return { ok: true }
  }
}
