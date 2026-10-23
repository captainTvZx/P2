-- Aktivite gönderisi türü: sürüm yayınlandığında notlar otomatik gönderi olur.
ALTER TABLE "ProjectPost" ADD COLUMN IF NOT EXISTS "kind" TEXT NOT NULL DEFAULT 'POST';
ALTER TABLE "ProjectPost" ADD COLUMN IF NOT EXISTS "releaseId" TEXT;
