-- Bir projede birden çok pano: her takım kendi kanbanını yönetir.
-- Sprint ve görevler artık projeye değil, projenin içindeki bir panoya bağlı.
CREATE TABLE "Board" (
  "id" TEXT NOT NULL,
  "organizationId" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "position" INTEGER NOT NULL DEFAULT 0,
  "createdById" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL,
  CONSTRAINT "Board_pkey" PRIMARY KEY ("id")
);
CREATE INDEX "Board_projectId_idx" ON "Board"("projectId");
CREATE INDEX "Board_organizationId_idx" ON "Board"("organizationId");
ALTER TABLE "Board" ADD CONSTRAINT "Board_organizationId_fkey" FOREIGN KEY ("organizationId") REFERENCES "Organization"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Board" ADD CONSTRAINT "Board_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Board" ADD CONSTRAINT "Board_createdById_fkey" FOREIGN KEY ("createdById") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- Görev ya da sprint'i olan her projeye "Genel" panosu açılır; eldeki kayıtlar
-- oraya taşınır, böylece kimsenin işi kaybolmaz.
INSERT INTO "Board" ("id", "organizationId", "projectId", "name", "position", "createdById", "updatedAt")
SELECT gen_random_uuid()::text, p."organizationId", p."id", 'Genel', 0, p."ownerId", CURRENT_TIMESTAMP
FROM "Project" p
WHERE p."organizationId" IS NOT NULL
  AND (EXISTS (SELECT 1 FROM "Task" t WHERE t."projectId" = p."id")
    OR EXISTS (SELECT 1 FROM "Sprint" s WHERE s."projectId" = p."id"));

ALTER TABLE "Sprint" ADD COLUMN "boardId" TEXT;
ALTER TABLE "Task" ADD COLUMN "boardId" TEXT;
UPDATE "Sprint" s SET "boardId" = (SELECT b."id" FROM "Board" b WHERE b."projectId" = s."projectId" ORDER BY b."position" ASC LIMIT 1);
UPDATE "Task" t SET "boardId" = (SELECT b."id" FROM "Board" b WHERE b."projectId" = t."projectId" ORDER BY b."position" ASC LIMIT 1);
DELETE FROM "Task" WHERE "boardId" IS NULL;
DELETE FROM "Sprint" WHERE "boardId" IS NULL;
ALTER TABLE "Sprint" ALTER COLUMN "boardId" SET NOT NULL;
ALTER TABLE "Task" ALTER COLUMN "boardId" SET NOT NULL;

CREATE INDEX "Sprint_boardId_idx" ON "Sprint"("boardId");
CREATE INDEX "Task_boardId_status_idx" ON "Task"("boardId", "status");
ALTER TABLE "Sprint" ADD CONSTRAINT "Sprint_boardId_fkey" FOREIGN KEY ("boardId") REFERENCES "Board"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Task" ADD CONSTRAINT "Task_boardId_fkey" FOREIGN KEY ("boardId") REFERENCES "Board"("id") ON DELETE CASCADE ON UPDATE CASCADE;
