-- Keşfet: projeler herkese açık (PUBLIC) ya da özel (PRIVATE) olabilir. Var olan projeler özel kalır;
-- açık projeler "Keşfet" ekranında listelenir ve davet kodu olmadan tek tıkla katılınır (yasaklar geçerli).
-- Göç yalnız ekler; tekrar çalıştırılsa da aynı sonucu verir.

ALTER TABLE "Project" ADD COLUMN IF NOT EXISTS "visibility" TEXT NOT NULL DEFAULT 'PRIVATE';
-- Keşfet kategorisi: GAME | DEV | COMMUNITY | EDUCATION | MUSIC | OTHER (boş: kategorisiz).
ALTER TABLE "Project" ADD COLUMN IF NOT EXISTS "category" TEXT;
-- Kartlarda adın altında görünen tek satırlık tanıtım.
ALTER TABLE "Project" ADD COLUMN IF NOT EXISTS "tagline" TEXT;

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'Project_visibility_check') THEN
    ALTER TABLE "Project" ADD CONSTRAINT "Project_visibility_check" CHECK ("visibility" IN ('PUBLIC', 'PRIVATE'));
  END IF;
END $$;

CREATE INDEX IF NOT EXISTS "Project_visibility_idx" ON "Project"("visibility") WHERE "deletedAt" IS NULL;

-- Haberler: gönderinin isteğe bağlı başlığı ve kapak görseli (yoksa ilk satır / projenin kapağı kullanılır).
ALTER TABLE "ProjectPost" ADD COLUMN IF NOT EXISTS "title" TEXT;
ALTER TABLE "ProjectPost" ADD COLUMN IF NOT EXISTS "coverUrl" TEXT;
CREATE INDEX IF NOT EXISTS "ProjectPost_createdAt_idx" ON "ProjectPost"("createdAt");
