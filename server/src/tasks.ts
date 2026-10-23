import { BadRequestException, Body, ConflictException, Controller, Delete, ForbiddenException, Get, Inject, NotFoundException, Param, Patch, Post, Req, UseGuards } from '@nestjs/common'
import { IsIn, IsInt, IsISO8601, IsOptional, IsString, Length, Max, Min } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { effectivePermissions, requirePermission, requireProjectAccess } from './permissions'
import { groupReactions } from './projects'
import { mentionedUserIds, recordActivity } from './activity'
import { canViewProject } from './permissions'
import { Dto } from './validation'

export const TASK_STATUSES = ['BACKLOG', 'TODO', 'IN_PROGRESS', 'REVIEW', 'DONE'] as const
export const TASK_PRIORITIES = ['LOW', 'MEDIUM', 'HIGH', 'URGENT'] as const
const SPRINT_STATUSES = ['PLANNED', 'ACTIVE', 'DONE'] as const
const COMMENT_EDIT_WINDOW = 2 * 60 * 60 * 1000

export class CommentDto {
  @IsString() @Length(1, 12000) content!: string
}

export class ReactDto {
  @IsString() @Length(1, 24) emoji!: string
}

export class BoardDto {
  @IsString() @Length(2, 60) name!: string
}

export class CreateSprintDto {
  @IsString() @Length(2, 60) name!: string
  @IsOptional() @IsString() @Length(0, 240) goal?: string
  @IsOptional() @IsISO8601() startsAt?: string
  @IsOptional() @IsISO8601() endsAt?: string
}

export class UpdateSprintDto {
  @IsOptional() @IsString() @Length(2, 60) name?: string
  @IsOptional() @IsString() @Length(0, 240) goal?: string
  @IsOptional() @IsString() @IsIn(SPRINT_STATUSES as unknown as string[]) status?: string
  @IsOptional() @IsISO8601() startsAt?: string
  @IsOptional() @IsISO8601() endsAt?: string
}

export class CreateTaskDto {
  @IsString() @Length(2, 160) title!: string
  // Açıklama sohbetteki gibi biçimli metin ve dosya ekleri taşıyabilir.
  @IsOptional() @IsString() @Length(0, 12000) description?: string
  @IsOptional() @IsString() @IsIn(TASK_STATUSES as unknown as string[]) status?: string
  @IsOptional() @IsString() @IsIn(TASK_PRIORITIES as unknown as string[]) priority?: string
  @IsOptional() @IsString() assigneeId?: string
  @IsOptional() @IsString() sprintId?: string
  @IsOptional() @IsInt() @Min(0) @Max(100) points?: number
  @IsOptional() @IsISO8601() dueAt?: string
}

export class UpdateTaskDto extends CreateTaskDto {
  @IsOptional() @IsString() @Length(2, 160) declare title: string
  @IsOptional() @IsInt() position?: number
}

const shape = {
  id: true, number: true, title: true, description: true, status: true, priority: true, points: true,
  position: true, assigneeId: true, createdById: true, sprintId: true, projectId: true, boardId: true, dueAt: true,
  createdAt: true, updatedAt: true,
} as const

const boardShape = { id: true, name: true, position: true, projectId: true, createdById: true, createdAt: true } as const

const commentShape = { id: true, taskId: true, authorId: true, content: true, createdAt: true, updatedAt: true } as const

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects/:projectId/boards')
export class TasksController {
  constructor(@Inject(PrismaService) private db: PrismaService, @Inject(RealtimeGateway) private stream: RealtimeGateway) {}

  // Panoyu herkes görür; görev açmak ve sprint yönetmek ayrı yetkilerde.
  private async member(organizationId: string, userId: string) {
    const membership = await this.db.organizationMember.findUnique({ where: { organizationId_userId: { organizationId, userId } } })
    if (!membership) throw new NotFoundException({ code: 'NOT_MEMBER', message: 'Bu kuruluşun üyesi değilsin.' })
    return membership
  }

  // Pano projeye ait: adresteki proje gerçekten bu kuruluşun mu?
  private async project(organizationId: string, projectId: string, userId: string) {
    await this.member(organizationId, userId)
    await requireProjectAccess(this.db, organizationId, userId, projectId)
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId, deletedAt: null }, select: { id: true, name: true } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    return project
  }

  // Bir projede birden çok pano olur (her takımın kendi tahtası); adresteki
  // pano gerçekten bu projenin mi?
  private async board(organizationId: string, projectId: string, boardId: string, userId: string) {
    await this.project(organizationId, projectId, userId)
    const board = await this.db.board.findFirst({ where: { id: boardId, projectId }, select: boardShape })
    if (!board) throw new NotFoundException({ code: 'BOARD_NOT_FOUND', message: 'Pano bulunamadı.' })
    return board
  }

  // Görev yalnızca kendi panosunun sprint'ine bağlanabilir.
  private async sprintOfBoard(boardId: string, sprintId: string) {
    const sprint = await this.db.sprint.findFirst({ where: { id: sprintId, boardId }, select: { id: true } })
    if (!sprint) throw new BadRequestException({ code: 'SPRINT_NOT_IN_BOARD', message: 'Sprint bu panoya ait değil.' })
    return sprint
  }

  // Kendi görevini herkes düzenleyip taşıyabilir; başkasınınkine board.manage gerekir.
  private async canEdit(organizationId: string, userId: string, task: { createdById: string; assigneeId: string | null }) {
    if (task.createdById === userId || task.assigneeId === userId) return true
    const effective = await effectivePermissions(this.db, organizationId, userId)
    return effective.permissions.includes('board.manage')
  }

  // Projedeki panolar. Hiç yoksa ilk giren için "Genel" panosu açılır, böylece
  // ekip boş ekranla karşılaşmaz.
  @Get()
  async boards(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    const boards = await this.db.board.findMany({ where: { projectId }, select: boardShape, orderBy: [{ position: 'asc' }, { createdAt: 'asc' }] })
    if (boards.length) return boards
    const first = await this.db.board.create({ data: { organizationId, projectId, name: 'Genel', position: 0, createdById: req.user.sub }, select: boardShape })
    void this.stream.emitProject(organizationId, projectId, 'board:updated', { board: first })
    return [first]
  }

  @Post()
  async createBoard(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Dto(BoardDto) dto: BoardDto, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const last = await this.db.board.findFirst({ where: { projectId }, orderBy: { position: 'desc' }, select: { position: true } })
    const board = await this.db.board.create({
      data: { organizationId, projectId, name: dto.name.trim(), position: (last?.position ?? -1) + 1, createdById: req.user.sub },
      select: boardShape,
    })
    void this.stream.emitProject(organizationId, projectId, 'board:updated', { board })
    return board
  }

  @Patch(':boardId')
  async renameBoard(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Dto(BoardDto) dto: BoardDto, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const board = await this.db.board.update({ where: { id: boardId }, data: { name: dto.name.trim() }, select: boardShape })
    void this.stream.emitProject(organizationId, projectId, 'board:updated', { board })
    return board
  }

  // Pano silinince sprint'leri ve görevleri de gider; projenin son panosu silinemez.
  @Delete(':boardId')
  async deleteBoard(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const total = await this.db.board.count({ where: { projectId } })
    if (total < 2) throw new BadRequestException({ code: 'LAST_BOARD', message: 'Projenin son panosu silinemez.' })
    await this.db.board.delete({ where: { id: boardId } })
    void this.stream.emitProject(organizationId, projectId, 'board:deleted', { boardId, projectId })
    return { ok: true }
  }

  @Get(':boardId')
  async content(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Req() req: any) {
    const board = await this.board(organizationId, projectId, boardId, req.user.sub)
    const [sprints, tasks] = await Promise.all([
      this.db.sprint.findMany({ where: { boardId }, orderBy: [{ status: 'asc' }, { createdAt: 'desc' }] }),
      this.db.task.findMany({ where: { boardId }, select: shape, orderBy: [{ position: 'asc' }, { createdAt: 'asc' }] }),
    ])
    return { board, sprints, tasks }
  }

  @Post(':boardId/sprints')
  async createSprint(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Dto(CreateSprintDto) dto: CreateSprintDto, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const sprint = await this.db.sprint.create({
      data: {
        organizationId, projectId, boardId, name: dto.name.trim(), goal: dto.goal?.trim() || null,
        startsAt: dto.startsAt ? new Date(dto.startsAt) : null, endsAt: dto.endsAt ? new Date(dto.endsAt) : null,
        createdById: req.user.sub,
      },
    })
    void this.stream.emitProject(organizationId, projectId, 'sprint:updated', { sprint })
    return sprint
  }

  @Patch(':boardId/sprints/:sprintId')
  async updateSprint(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('sprintId') sprintId: string, @Dto(UpdateSprintDto) dto: UpdateSprintDto, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const current = await this.db.sprint.findFirst({ where: { id: sprintId, boardId } })
    if (!current) throw new NotFoundException({ code: 'SPRINT_NOT_FOUND', message: 'Sprint bulunamadı.' })
    // Aynı panoda aynı anda tek sprint aktif olur: yenisi başlarken öncekiler planlanana döner.
    if (dto.status === 'ACTIVE') {
      await this.db.sprint.updateMany({ where: { boardId, status: 'ACTIVE', NOT: { id: sprintId } }, data: { status: 'PLANNED' } })
    }
    const sprint = await this.db.sprint.update({
      where: { id: sprintId },
      data: {
        ...(dto.name !== undefined ? { name: dto.name.trim() } : {}),
        ...(dto.goal !== undefined ? { goal: dto.goal.trim() || null } : {}),
        ...(dto.status !== undefined ? { status: dto.status } : {}),
        ...(dto.startsAt !== undefined ? { startsAt: new Date(dto.startsAt) } : {}),
        ...(dto.endsAt !== undefined ? { endsAt: new Date(dto.endsAt) } : {}),
        ...(dto.status === 'ACTIVE' && !current.startsAt ? { startsAt: new Date() } : {}),
      },
    })
    void this.stream.emitProject(organizationId, projectId, 'sprint:updated', { sprint, reload: dto.status === 'ACTIVE' })
    return sprint
  }

  @Delete(':boardId/sprints/:sprintId')
  async deleteSprint(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('sprintId') sprintId: string, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const sprint = await this.db.sprint.findFirst({ where: { id: sprintId, boardId } })
    if (!sprint) throw new NotFoundException({ code: 'SPRINT_NOT_FOUND', message: 'Sprint bulunamadı.' })
    // Görevler silinmez, backlog'a düşer.
    await this.db.task.updateMany({ where: { sprintId }, data: { sprintId: null } })
    await this.db.sprint.delete({ where: { id: sprintId } })
    void this.stream.emitProject(organizationId, projectId, 'sprint:deleted', { sprintId, projectId, boardId })
    return { ok: true }
  }

  @Post(':boardId/tasks')
  async createTask(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Dto(CreateTaskDto) dto: CreateTaskDto, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'task.create')
    if (dto.assigneeId) await this.member(organizationId, dto.assigneeId)
    if (dto.sprintId) await this.sprintOfBoard(boardId, dto.sprintId)
    // Görev numarası proje içinde sıralı: aynı projedeki tüm panolar tek
    // numara dizisini paylaşır, böylece #12 her zaman tek bir işi gösterir.
    const last = await this.db.task.findFirst({ where: { projectId }, orderBy: { number: 'desc' }, select: { number: true } })
    const first = await this.db.task.findFirst({ where: { boardId, status: dto.status || 'TODO' }, orderBy: { position: 'asc' }, select: { position: true } })
    const task = await this.db.task.create({
      data: {
        organizationId, projectId, boardId, number: (last?.number || 0) + 1, title: dto.title.trim(),
        description: dto.description?.trim() || null,
        status: dto.status || 'TODO', priority: dto.priority || 'MEDIUM',
        assigneeId: dto.assigneeId || null, sprintId: dto.sprintId || null,
        points: dto.points ?? null, dueAt: dto.dueAt ? new Date(dto.dueAt) : null,
        position: (first?.position ?? 0) - 1, createdById: req.user.sub,
      },
      select: shape,
    })
    if(task.description){const mentioned=await mentionedUserIds(this.db,organizationId,task.description),targets:string[]=[];for(const userId of mentioned)if(await canViewProject(this.db,organizationId,userId,projectId))targets.push(userId);await recordActivity(this.db,this.stream,targets.map(userId=>({organizationId,userId,actorId:req.user.sub,kind:'MENTION' as const,preview:task.description||task.title,projectId,boardId,taskId:task.id})))}
    // Görev başkasına atandıysa atanan kişiye "sana bir görev atadı" etkinliği (oyun içi overlay bunu toast yapar).
    if(task.assigneeId&&task.assigneeId!==req.user.sub)await recordActivity(this.db,this.stream,[{organizationId,userId:task.assigneeId,actorId:req.user.sub,kind:'ASSIGNED',preview:task.title,projectId,boardId,taskId:task.id}])
    void this.stream.emitProject(organizationId, projectId, 'task:updated', { task })
    return task
  }

  @Patch(':boardId/tasks/:taskId')
  async updateTask(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('taskId') taskId: string, @Dto(UpdateTaskDto) dto: UpdateTaskDto, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    if (dto.sprintId) await this.sprintOfBoard(boardId, dto.sprintId)
    const current = await this.db.task.findFirst({ where: { id: taskId, boardId }, select: { createdById: true, assigneeId: true, description:true } })
    if (!current) throw new NotFoundException({ code: 'TASK_NOT_FOUND', message: 'Görev bulunamadı.' })
    if (!(await this.canEdit(organizationId, req.user.sub, current))) {
      throw new ForbiddenException({ code: 'NO_PERMISSION', message: 'Bu görevi düzenleme yetkin yok.' })
    }
    if (dto.assigneeId) await this.member(organizationId, dto.assigneeId)
    const task = await this.db.task.update({
      where: { id: taskId },
      data: {
        ...(dto.title !== undefined ? { title: dto.title.trim() } : {}),
        ...(dto.description !== undefined ? { description: dto.description.trim() || null } : {}),
        ...(dto.status !== undefined ? { status: dto.status } : {}),
        ...(dto.priority !== undefined ? { priority: dto.priority } : {}),
        ...(dto.points !== undefined ? { points: dto.points } : {}),
        ...(dto.position !== undefined ? { position: dto.position } : {}),
        ...(dto.dueAt !== undefined ? { dueAt: dto.dueAt ? new Date(dto.dueAt) : null } : {}),
        // Boş dizge "kimseye atanmadı" / "backlog" anlamına gelir.
        ...(dto.assigneeId !== undefined ? { assigneeId: dto.assigneeId || null } : {}),
        ...(dto.sprintId !== undefined ? { sprintId: dto.sprintId || null } : {}),
      },
      select: shape,
    })
    if(dto.description!==undefined){const[before,after]=await Promise.all([mentionedUserIds(this.db,organizationId,current.description||''),mentionedUserIds(this.db,organizationId,task.description||'')]),targets:string[]=[];for(const userId of after.filter(id=>!before.includes(id)))if(await canViewProject(this.db,organizationId,userId,projectId))targets.push(userId);await recordActivity(this.db,this.stream,targets.map(userId=>({organizationId,userId,actorId:req.user.sub,kind:'MENTION' as const,preview:task.description||task.title,projectId,boardId,taskId:task.id})))}
    // Atanan kişi değiştiyse ve yeni atanan aktör değilse: "sana bir görev atadı".
    if(dto.assigneeId!==undefined&&task.assigneeId&&task.assigneeId!==current.assigneeId&&task.assigneeId!==req.user.sub)await recordActivity(this.db,this.stream,[{organizationId,userId:task.assigneeId,actorId:req.user.sub,kind:'ASSIGNED',preview:task.title,projectId,boardId,taskId:task.id}])
    void this.stream.emitProject(organizationId, projectId, 'task:updated', { task })
    return task
  }

  @Delete(':boardId/tasks/:taskId')
  async deleteTask(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('taskId') taskId: string, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    const current = await this.db.task.findFirst({ where: { id: taskId, boardId }, select: { createdById: true, assigneeId: true } })
    if (!current) throw new NotFoundException({ code: 'TASK_NOT_FOUND', message: 'Görev bulunamadı.' })
    if (!(await this.canEdit(organizationId, req.user.sub, current))) {
      throw new ForbiddenException({ code: 'NO_PERMISSION', message: 'Bu görevi silme yetkin yok.' })
    }
    await this.db.task.delete({ where: { id: taskId } })
    void this.stream.emitProject(organizationId, projectId, 'task:deleted', { taskId, projectId, boardId })
    return { ok: true }
  }

  // --- Görev yorumları: görevin altındaki konuşma ---
  @Get(':boardId/tasks/:taskId/comments')
  async comments(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('taskId') taskId: string, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    const task = await this.db.task.findFirst({ where: { id: taskId, boardId }, select: { id: true } })
    if (!task) throw new NotFoundException({ code: 'TASK_NOT_FOUND', message: 'Görev bulunamadı.' })
    const [rows, reactions] = await Promise.all([
      this.db.taskComment.findMany({ where: { taskId }, select: commentShape, orderBy: { createdAt: 'asc' } }),
      this.db.taskCommentReaction.findMany({ where: { comment: { taskId } }, orderBy: { createdAt: 'asc' }, select: { commentId: true, emoji: true, userId: true } }),
    ])
    const me = req.user.sub as string
    return rows.map(row => ({ ...row, reactions: groupReactions(reactions.filter(item => item.commentId === row.id), me) }))
  }

  // Panoyu görebilen herkes yorum yazabilir; görev açmak gibi ayrı bir yetki
  // istemiyoruz, konuşma panonun doğal parçası.
  @Post(':boardId/tasks/:taskId/comments')
  async addComment(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('taskId') taskId: string, @Dto(CommentDto) dto: CommentDto, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    const task = await this.db.task.findFirst({ where: { id: taskId, boardId }, select: { id: true } })
    if (!task) throw new NotFoundException({ code: 'TASK_NOT_FOUND', message: 'Görev bulunamadı.' })
    const comment = await this.db.taskComment.create({
      data: { taskId, authorId: req.user.sub, content: dto.content.trim() },
      select: commentShape,
    })
    const mentioned=await mentionedUserIds(this.db,organizationId,dto.content)
    const targets:string[]=[]
    for(const userId of mentioned)if(await canViewProject(this.db,organizationId,userId,projectId))targets.push(userId)
    await recordActivity(this.db,this.stream,targets.map(userId=>({organizationId,userId,actorId:req.user.sub,kind:'MENTION' as const,preview:dto.content,projectId,boardId,taskId,messageId:comment.id})))
    void this.stream.emitProject(organizationId, projectId, 'task:comment', { comment, projectId, boardId })
    return comment
  }

  // Yorum tepkileri: aynı emoji ikinci kez gönderilince geri alınır.
  @Post(':boardId/tasks/:taskId/comments/:commentId/reactions')
  async reactComment(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('taskId') taskId: string, @Param('commentId') commentId: string, @Dto(ReactDto) dto: ReactDto, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    const comment = await this.db.taskComment.findFirst({ where: { id: commentId, taskId }, select: { id: true } })
    if (!comment) throw new NotFoundException({ code: 'COMMENT_NOT_FOUND', message: 'Yorum bulunamadı.' })
    const emoji = dto.emoji.trim()
    const userId = req.user.sub as string
    const existing = await this.db.taskCommentReaction.findUnique({ where: { commentId_userId_emoji: { commentId, userId, emoji } } })
    if (existing) await this.db.taskCommentReaction.delete({ where: { id: existing.id } })
    else await this.db.taskCommentReaction.create({ data: { commentId, userId, emoji } })
    const rows = await this.db.taskCommentReaction.findMany({ where: { commentId }, orderBy: { createdAt: 'asc' }, select: { emoji: true, userId: true } })
    void this.stream.emitProject(organizationId, projectId, 'task:comment:reactions', { commentId, taskId, projectId, boardId, rows })
    return groupReactions(rows, userId)
  }

  // Kendi yorumunu herkes düzenler/siler; başkasınınkini board.manage siler.
  @Patch(':boardId/tasks/:taskId/comments/:commentId')
  async editComment(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('taskId') taskId: string, @Param('commentId') commentId: string, @Dto(CommentDto) dto: CommentDto, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    const current = await this.db.taskComment.findFirst({ where: { id: commentId, taskId }, select: { authorId: true, createdAt: true, content:true } })
    if (!current) throw new NotFoundException({ code: 'COMMENT_NOT_FOUND', message: 'Yorum bulunamadı.' })
    if (current.authorId !== req.user.sub) throw new ForbiddenException({ code: 'NO_PERMISSION', message: 'Yalnızca kendi yorumunu düzenleyebilirsin.' })
    if (Date.now() - current.createdAt.getTime() > COMMENT_EDIT_WINDOW) throw new ConflictException({ code: 'COMMENT_EDIT_WINDOW_EXPIRED', message: 'Yorumlar yalnızca ilk 2 saat içinde düzenlenebilir.' })
    const comment = await this.db.taskComment.update({ where: { id: commentId }, data: { content: dto.content.trim() }, select: commentShape })
    const [before,after]=await Promise.all([mentionedUserIds(this.db,organizationId,current.content),mentionedUserIds(this.db,organizationId,dto.content)])
    const newlyMentioned=after.filter(userId=>!before.includes(userId)),targets:string[]=[]
    for(const userId of newlyMentioned)if(await canViewProject(this.db,organizationId,userId,projectId))targets.push(userId)
    await recordActivity(this.db,this.stream,targets.map(userId=>({organizationId,userId,actorId:req.user.sub,kind:'MENTION' as const,preview:dto.content,projectId,boardId,taskId,messageId:comment.id})))
    void this.stream.emitProject(organizationId, projectId, 'task:comment', { comment, projectId, boardId })
    return comment
  }

  @Delete(':boardId/tasks/:taskId/comments/:commentId')
  async removeComment(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('taskId') taskId: string, @Param('commentId') commentId: string, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    const current = await this.db.taskComment.findFirst({ where: { id: commentId, taskId }, select: { authorId: true, createdAt: true } })
    if (!current) throw new NotFoundException({ code: 'COMMENT_NOT_FOUND', message: 'Yorum bulunamadı.' })
    const effective = await effectivePermissions(this.db, organizationId, req.user.sub)
    const unlimited = effective.permissions.includes('message.delete_unlimited')
    const ownAndFresh=current.authorId===req.user.sub&&Date.now()-current.createdAt.getTime()<=COMMENT_EDIT_WINDOW
    if (!ownAndFresh && !unlimited) {
      if (!effective.permissions.includes('board.manage')) {
        if(current.authorId===req.user.sub)throw new ConflictException({ code: 'COMMENT_DELETE_WINDOW_EXPIRED', message: 'Yorumlar yalnızca ilk 2 saat içinde silinebilir.' })
        throw new ForbiddenException({ code: 'NO_PERMISSION', message: 'Bu yorumu silme yetkin yok.' })
      }
    }
    await this.db.taskComment.delete({ where: { id: commentId } })
    void this.stream.emitProject(organizationId, projectId, 'task:comment:deleted', { commentId, taskId, projectId, boardId })
    return { ok: true }
  }

  // Sütun içinde sıralama: sürükle bırak sonrası yeni düzen tek istekte yazılır.
  @Patch(':boardId/tasks/:taskId/move')
  async moveTask(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('boardId') boardId: string, @Param('taskId') taskId: string, @Body() body: { status?: string; order?: string[] }, @Req() req: any) {
    await this.board(organizationId, projectId, boardId, req.user.sub)
    const current = await this.db.task.findFirst({ where: { id: taskId, boardId }, select: { createdById: true, assigneeId: true } })
    if (!current) throw new NotFoundException({ code: 'TASK_NOT_FOUND', message: 'Görev bulunamadı.' })
    if (!(await this.canEdit(organizationId, req.user.sub, current))) {
      throw new ForbiddenException({ code: 'NO_PERMISSION', message: 'Bu görevi taşıma yetkin yok.' })
    }
    const status = String(body?.status || '')
    if (status && !TASK_STATUSES.includes(status as typeof TASK_STATUSES[number])) {
      throw new BadRequestException({ code: 'INVALID_STATUS', message: 'Geçersiz sütun.' })
    }
    const order = Array.isArray(body?.order) ? body.order.slice(0, 500) : []
    await this.db.$transaction([
      ...(status ? [this.db.task.update({ where: { id: taskId }, data: { status } })] : []),
      ...order.map((id, index) => this.db.task.updateMany({ where: { id, boardId }, data: { position: index } })),
    ])
    const tasks = await this.db.task.findMany({ where: { boardId, id: { in: [taskId, ...order] } }, select: shape })
    void this.stream.emitProject(organizationId, projectId, 'task:moved', { tasks, projectId, boardId })
    return { ok: true, tasks }
  }
}
