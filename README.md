# P2 Desktop 0.3.8

React/Electron masaüstü istemcisi, NestJS API ve PostgreSQL tabanlı geliştirici oyun platformu.

## Komutlar

- `.env.example` dosyasını `.env` olarak kopyalayıp PostgreSQL bağlantısını ayarlayın
- `npm run db:generate` — Prisma istemcisini üretir
- `npm run db:migrate` — veritabanı migration'larını uygular
- `npm run dev` — web, API ve Electron geliştirme ortamını birlikte açar
- `npm run test:server` — backend testlerini çalıştırır
- `npm run build` — paketlenmiş uygulamayı üretir
- `npm run dist` — Windows kurulum dosyasını `release-package/` içine üretir
