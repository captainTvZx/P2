# P2 Desktop 0.5.19

React/Electron masaüstü istemcisi, NestJS API ve PostgreSQL tabanlı geliştirici oyun platformu.

## Komutlar

- `.env.example` dosyasını `.env` olarak kopyalayıp PostgreSQL bağlantısını ayarlayın
- `npm run db:generate` — Prisma istemcisini üretir
- `npm run db:migrate` — veritabanı migration'larını uygular
- `npm run dev` — web, API ve Electron geliştirme ortamını birlikte açar
- `npm run test:server` — backend testlerini çalıştırır
- `npm run build` — paketlenmiş uygulamayı üretir
- `npm run dist` — Windows kurulum dosyasını `release-package/` içine üretir
- `npm run publish:update` — yeni sürümü paketleyip sunucunun güncelleme klasörüne (`%ProgramData%\P2\updates`) koyar; açık uygulamalar sürümü anlık duyuruyla, duyuru kaçırılırsa en geç 5 dakika içinde görür
- `npm run test:updates` — güncelleme ucunun testleri

## Yeni sürüm yayınlama

1. `package.json` ve `server/package.json` içindeki `version` alanını yükselt.
2. Sunucuyu barındıran bilgisayarda `npm run publish:update` çalıştır.
3. Hepsi bu: açık olan istemciler yeni sürümü indirip sessizce kurar (kurulum sihirbazı çıkmaz). Uygulama açılırken güncelleme bulursa ortadaki pencerede indirip kurar ve yeni sürümle açılır; çalışırken bulursa "Yeni sürüm hazır — yeniden başlat" şeridini gösterir. Kullanıcı isterse hemen, istemezse uygulamayı kapattığında kurulur.
