import 'reflect-metadata'
import { ValidationPipe } from '@nestjs/common'
import { NestFactory } from '@nestjs/core'
import { config } from 'dotenv'
import { AppModule } from './app'
import { validationException } from './validation'

config({ path: process.env.P2_ENV_PATH || '.env' })
async function bootstrap() {
  const app = await NestFactory.create(AppModule)
  app.enableCors({ origin: true, credentials: false })
  app.useGlobalPipes(new ValidationPipe({ whitelist: true, transform: true, exceptionFactory: validationException }))
  await app.listen(Number(process.env.PORT || 4000), '127.0.0.1')
}
bootstrap()
