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
  // Yerel ağdaki diğer cihazlar (telefon) da bağlanabilsin diye tüm arayüzler dinlenir.
  await app.listen(Number(process.env.PORT || 4000), process.env.HOST || '0.0.0.0')
}
bootstrap()
