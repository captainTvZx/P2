-- Kütüphanede uygulama olarak görünme bayrağı. Yayınlanmış sürümü olan projeler geriye dönük olarak açık gelir.
ALTER TABLE "Project" ADD COLUMN IF NOT EXISTS "libraryEnabled" BOOLEAN NOT NULL DEFAULT false;
UPDATE "Project" SET "libraryEnabled" = true WHERE "id" IN (SELECT DISTINCT "projectId" FROM "Release" WHERE "status" = 'PUBLISHED');
