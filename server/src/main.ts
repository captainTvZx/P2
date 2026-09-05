import 'reflect-metadata'
import { ValidationPipe } from '@nestjs/common'
import { NestFactory } from '@nestjs/core'
import { NestExpressApplication } from '@nestjs/platform-express'
import { config } from 'dotenv'
import { AppModule } from './app'
import { validationException } from './validation'

config({ path: process.env.P2_ENV_PATH || '.env' })
async function bootstrap() {
  const app = await NestFactory.create<NestExpressApplication>(AppModule,{bodyParser:false})
  // Dosyalar JSON içindeki base64 veriyle gelir; %33 kodlama payıyla 250 MB dosyayı kabul eder.
  app.useBodyParser('json',{limit:'340mb'})
  app.useBodyParser('urlencoded',{limit:'1mb',extended:true})
  app.enableCors({ origin: true, credentials: false })
  app.useGlobalPipes(new ValidationPipe({ whitelist: true, transform: true, exceptionFactory: validationException }))
  // Yerel ağdaki diğer cihazlar (telefon) da bağlanabilsin diye tüm arayüzler dinlenir.
  await app.listen(Number(process.env.PORT || 4000), process.env.HOST || '0.0.0.0')
}
bootstrap()
