import { BadRequestException, Body, Controller, Delete, ForbiddenException, Get, Inject, NotFoundException, Param, Patch, Post, Req, UseGuards } from '@nestjs/common'
import { IsIn, IsInt, IsISO8601, IsOptional, IsString, Length, Max, Min } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { RealtimeGateway } from './realtime'
import { effectivePermissions, requirePermission } from './permissions'
import { Dto } from './validation'

export const TASK_STATUSES = ['BACKLOG', 'TODO', 'IN_PROGRESS', 'REVIEW', 'DONE'] as const
export const TASK_PRIORITIES = ['LOW', 'MEDIUM', 'HIGH', 'URGENT'] as const
const SPRINT_STATUSES = ['PLANNED', 'ACTIVE', 'DONE'] as const

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
  @IsOptional() @IsString() @Length(0, 4000) description?: string
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
  position: true, assigneeId: true, createdById: true, sprintId: true, projectId: true, dueAt: true,
  createdAt: true, updatedAt: true,
} as const

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects/:projectId/board')
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
    const project = await this.db.project.findFirst({ where: { id: projectId, organizationId, deletedAt: null }, select: { id: true, name: true } })
    if (!project) throw new NotFoundException({ code: 'PROJECT_NOT_FOUND', message: 'Proje bulunamadı.' })
    return project
  }

  // Kendi görevini herkes düzenleyip taşıyabilir; başkasınınkine board.manage gerekir.
  private async canEdit(organizationId: string, userId: string, task: { createdById: string; assigneeId: string | null }) {
    if (task.createdById === userId || task.assigneeId === userId) return true
    const effective = await effectivePermissions(this.db, organizationId, userId)
    return effective.permissions.includes('board.manage')
  }

  @Get()
  async board(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    const [sprints, tasks] = await Promise.all([
      this.db.sprint.findMany({ where: { projectId }, orderBy: [{ status: 'asc' }, { createdAt: 'desc' }] }),
      this.db.task.findMany({ where: { projectId }, select: shape, orderBy: [{ position: 'asc' }, { createdAt: 'asc' }] }),
    ])
    return { sprints, tasks }
  }

  @Post('sprints')
  async createSprint(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Dto(CreateSprintDto) dto: CreateSprintDto, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const sprint = await this.db.sprint.create({
      data: {
        organizationId, projectId, name: dto.name.trim(), goal: dto.goal?.trim() || null,
        startsAt: dto.startsAt ? new Date(dto.startsAt) : null, endsAt: dto.endsAt ? new Date(dto.endsAt) : null,
        createdById: req.user.sub,
      },
    })
    this.stream.emit(organizationId, 'sprint:updated', { sprint })
    return sprint
  }

  @Patch('sprints/:sprintId')
  async updateSprint(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('sprintId') sprintId: string, @Dto(UpdateSprintDto) dto: UpdateSprintDto, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const current = await this.db.sprint.findFirst({ where: { id: sprintId, projectId } })
    if (!current) throw new NotFoundException({ code: 'SPRINT_NOT_FOUND', message: 'Sprint bulunamadı.' })
    // Aynı projede aynı anda tek sprint aktif olur: yenisi başlarken öncekiler planlanana döner.
    if (dto.status === 'ACTIVE') {
      await this.db.sprint.updateMany({ where: { projectId, status: 'ACTIVE', NOT: { id: sprintId } }, data: { status: 'PLANNED' } })
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
    this.stream.emit(organizationId, 'sprint:updated', { sprint, reload: dto.status === 'ACTIVE' })
    return sprint
  }

  @Delete('sprints/:sprintId')
  async deleteSprint(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('sprintId') sprintId: string, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'board.manage')
    const sprint = await this.db.sprint.findFirst({ where: { id: sprintId, projectId } })
    if (!sprint) throw new NotFoundException({ code: 'SPRINT_NOT_FOUND', message: 'Sprint bulunamadı.' })
    // Görevler silinmez, backlog'a düşer.
    await this.db.task.updateMany({ where: { sprintId }, data: { sprintId: null } })
    await this.db.sprint.delete({ where: { id: sprintId } })
    this.stream.emit(organizationId, 'sprint:deleted', { sprintId, projectId })
    return { ok: true }
  }

  @Post('tasks')
  async createTask(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Dto(CreateTaskDto) dto: CreateTaskDto, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    await requirePermission(this.db, organizationId, req.user.sub, 'task.create')
    if (dto.assigneeId) await this.member(organizationId, dto.assigneeId)
    // Görev numarası proje içinde sıralı: her projede #1'den başlar.
    const last = await this.db.task.findFirst({ where: { projectId }, orderBy: { number: 'desc' }, select: { number: true } })
    const first = await this.db.task.findFirst({ where: { projectId, status: dto.status || 'TODO' }, orderBy: { position: 'asc' }, select: { position: true } })
    const task = await this.db.task.create({
      data: {
        organizationId, projectId, number: (last?.number || 0) + 1, title: dto.title.trim(),
        description: dto.description?.trim() || null,
        status: dto.status || 'TODO', priority: dto.priority || 'MEDIUM',
        assigneeId: dto.assigneeId || null, sprintId: dto.sprintId || null,
        points: dto.points ?? null, dueAt: dto.dueAt ? new Date(dto.dueAt) : null,
        position: (first?.position ?? 0) - 1, createdById: req.user.sub,
      },
      select: shape,
    })
    this.stream.emit(organizationId, 'task:updated', { task })
    return task
  }

  @Patch('tasks/:taskId')
  async updateTask(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('taskId') taskId: string, @Dto(UpdateTaskDto) dto: UpdateTaskDto, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    const current = await this.db.task.findFirst({ where: { id: taskId, projectId }, select: { createdById: true, assigneeId: true } })
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
    this.stream.emit(organizationId, 'task:updated', { task })
    return task
  }

  @Delete('tasks/:taskId')
  async deleteTask(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('taskId') taskId: string, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    const current = await this.db.task.findFirst({ where: { id: taskId, projectId }, select: { createdById: true, assigneeId: true } })
    if (!current) throw new NotFoundException({ code: 'TASK_NOT_FOUND', message: 'Görev bulunamadı.' })
    if (!(await this.canEdit(organizationId, req.user.sub, current))) {
      throw new ForbiddenException({ code: 'NO_PERMISSION', message: 'Bu görevi silme yetkin yok.' })
    }
    await this.db.task.delete({ where: { id: taskId } })
    this.stream.emit(organizationId, 'task:deleted', { taskId, projectId })
    return { ok: true }
  }

  // Sütun içinde sıralama: sürükle bırak sonrası yeni düzen tek istekte yazılır.
  @Patch('tasks/:taskId/move')
  async moveTask(@Param('organizationId') organizationId: string, @Param('projectId') projectId: string, @Param('taskId') taskId: string, @Body() body: { status?: string; order?: string[] }, @Req() req: any) {
    await this.project(organizationId, projectId, req.user.sub)
    const current = await this.db.task.findFirst({ where: { id: taskId, projectId }, select: { createdById: true, assigneeId: true } })
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
      ...order.map((id, index) => this.db.task.updateMany({ where: { id, projectId }, data: { position: index } })),
    ])
    const tasks = await this.db.task.findMany({ where: { projectId, id: { in: [taskId, ...order] } }, select: shape })
    this.stream.emit(organizationId, 'task:moved', { tasks, projectId })
    return { ok: true, tasks }
  }
}
