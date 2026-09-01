import { Injectable, OnModuleDestroy, OnModuleInit } from '@nestjs/common'
import { PrismaClient } from '@prisma/client'

@Injectable()
export class PrismaService extends PrismaClient implements OnModuleInit, OnModuleDestroy {
  private cleanupTimer?: NodeJS.Timeout
  async onModuleInit() {
    await this.$connect()
    const cleanup = async () => {
      const now = new Date()
      await this.projectChannel.deleteMany({ where: { purgeAt: { lte: now }, category: { deletedAt: null } } })
      await this.channelCategory.deleteMany({ where: { purgeAt: { lte: now }, project: { deletedAt: null } } })
      await this.project.deleteMany({ where: { purgeAt: { lte: now } } })
    }
    await cleanup().catch(() => undefined)
    this.cleanupTimer = setInterval(() => { void cleanup().catch(() => undefined) }, 60 * 60 * 1000)
    this.cleanupTimer.unref()
  }
  async onModuleDestroy() { if (this.cleanupTimer) clearInterval(this.cleanupTimer); await this.$disconnect() }
}
