-- Proje ekran görüntüleri: overlay F12 çekimleri ve Kütüphane "Ekran görüntüleri" sekmesi.
CREATE TABLE IF NOT EXISTS "ProjectScreenshot" (
  "id" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "uploadPath" TEXT NOT NULL,
  "width" INTEGER NOT NULL,
  "height" INTEGER NOT NULL,
  "size" INTEGER NOT NULL DEFAULT 0,
  "caption" TEXT,
  "releaseId" TEXT,
  "takenAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProjectScreenshot_pkey" PRIMARY KEY ("id")
);
CREATE INDEX IF NOT EXISTS "ProjectScreenshot_projectId_takenAt_idx" ON "ProjectScreenshot"("projectId", "takenAt");
CREATE INDEX IF NOT EXISTS "ProjectScreenshot_userId_projectId_idx" ON "ProjectScreenshot"("userId", "projectId");
ALTER TABLE "ProjectScreenshot" DROP CONSTRAINT IF EXISTS "ProjectScreenshot_projectId_fkey";
ALTER TABLE "ProjectScreenshot" ADD CONSTRAINT "ProjectScreenshot_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ProjectScreenshot" DROP CONSTRAINT IF EXISTS "ProjectScreenshot_userId_fkey";
ALTER TABLE "ProjectScreenshot" ADD CONSTRAINT "ProjectScreenshot_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ProjectScreenshot" DROP CONSTRAINT IF EXISTS "ProjectScreenshot_releaseId_fkey";
ALTER TABLE "ProjectScreenshot" ADD CONSTRAINT "ProjectScreenshot_releaseId_fkey" FOREIGN KEY ("releaseId") REFERENCES "Release"("id") ON DELETE SET NULL ON UPDATE CASCADE;
