-- Proje notları: overlay "Notlar" penceresi ve Kütüphane "Notlar" sekmesi (özel / ekip).
CREATE TABLE IF NOT EXISTS "ProjectNote" (
  "id" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "authorId" TEXT NOT NULL,
  "title" TEXT NOT NULL DEFAULT '',
  "body" TEXT NOT NULL DEFAULT '',
  "visibility" TEXT NOT NULL DEFAULT 'PRIVATE',
  "pinned" BOOLEAN NOT NULL DEFAULT false,
  "version" INTEGER NOT NULL DEFAULT 1,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProjectNote_pkey" PRIMARY KEY ("id")
);
CREATE INDEX IF NOT EXISTS "ProjectNote_projectId_visibility_updatedAt_idx" ON "ProjectNote"("projectId", "visibility", "updatedAt");
CREATE INDEX IF NOT EXISTS "ProjectNote_authorId_projectId_idx" ON "ProjectNote"("authorId", "projectId");
ALTER TABLE "ProjectNote" DROP CONSTRAINT IF EXISTS "ProjectNote_projectId_fkey";
ALTER TABLE "ProjectNote" ADD CONSTRAINT "ProjectNote_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ProjectNote" DROP CONSTRAINT IF EXISTS "ProjectNote_authorId_fkey";
ALTER TABLE "ProjectNote" ADD CONSTRAINT "ProjectNote_authorId_fkey" FOREIGN KEY ("authorId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
