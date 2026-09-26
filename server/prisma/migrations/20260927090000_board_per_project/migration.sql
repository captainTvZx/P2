-- Pano artık kuruluşun değil, projenin: her projenin kendi sprint'leri, kendi
-- görevleri ve kendi #1'den başlayan numaraları var.

-- Projesiz kalmış kayıtlar (pano ilk sürümünde proje seçimi yoktu) kuruluşun
-- en eski projesine taşınır; hiç proje yoksa silinir.
UPDATE "Sprint" s SET "projectId" = (
  SELECT p."id" FROM "Project" p
  WHERE p."organizationId" = s."organizationId" AND p."deletedAt" IS NULL
  ORDER BY p."createdAt" ASC LIMIT 1
) WHERE s."projectId" IS NULL;
UPDATE "Task" t SET "projectId" = (
  SELECT p."id" FROM "Project" p
  WHERE p."organizationId" = t."organizationId" AND p."deletedAt" IS NULL
  ORDER BY p."createdAt" ASC LIMIT 1
) WHERE t."projectId" IS NULL;
DELETE FROM "Task" WHERE "projectId" IS NULL;
DELETE FROM "Sprint" WHERE "projectId" IS NULL;

-- Görev numaraları proje içinde yeniden sıralanır (PROJE #1, #2 …).
WITH sirali AS (
  SELECT "id", ROW_NUMBER() OVER (PARTITION BY "projectId" ORDER BY "number" ASC, "createdAt" ASC) AS "sira"
  FROM "Task"
)
UPDATE "Task" t SET "number" = sirali."sira" FROM sirali WHERE t."id" = sirali."id";

ALTER TABLE "Sprint" ALTER COLUMN "projectId" SET NOT NULL;
ALTER TABLE "Task" ALTER COLUMN "projectId" SET NOT NULL;

DROP INDEX IF EXISTS "Task_organizationId_number_key";
CREATE UNIQUE INDEX "Task_projectId_number_key" ON "Task"("projectId", "number");
CREATE INDEX IF NOT EXISTS "Task_projectId_status_idx" ON "Task"("projectId", "status");
CREATE INDEX IF NOT EXISTS "Sprint_projectId_idx" ON "Sprint"("projectId");

ALTER TABLE "Sprint" ADD CONSTRAINT "Sprint_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Task" ADD CONSTRAINT "Task_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
