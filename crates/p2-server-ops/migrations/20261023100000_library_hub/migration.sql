-- Kütüphane: canlı sürüm, oynama süresi, aktivite gönderileri, başarımlar, incelemeler.
ALTER TABLE "Release" ADD COLUMN IF NOT EXISTS "liveAt" TIMESTAMP(3);
CREATE INDEX IF NOT EXISTS "Release_projectId_liveAt_idx" ON "Release"("projectId", "liveAt");
-- Mevcut veri: her kategorinin en son yayınlanmış sürümü canlı sayılır.
UPDATE "Release" r SET "liveAt" = r."publishedAt"
WHERE r."status" = 'PUBLISHED' AND r."liveAt" IS NULL AND r."publishedAt" = (
  SELECT MAX(o."publishedAt") FROM "Release" o WHERE o."projectId" = r."projectId" AND o."channel" = r."channel" AND o."status" = 'PUBLISHED'
);

ALTER TABLE "ReleaseInstall" ADD COLUMN IF NOT EXISTS "playSeconds" INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS "ProjectPost" (
  "id" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "authorId" TEXT NOT NULL,
  "content" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProjectPost_pkey" PRIMARY KEY ("id")
);
CREATE INDEX IF NOT EXISTS "ProjectPost_projectId_createdAt_idx" ON "ProjectPost"("projectId", "createdAt");
ALTER TABLE "ProjectPost" DROP CONSTRAINT IF EXISTS "ProjectPost_projectId_fkey";
ALTER TABLE "ProjectPost" ADD CONSTRAINT "ProjectPost_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ProjectPost" DROP CONSTRAINT IF EXISTS "ProjectPost_authorId_fkey";
ALTER TABLE "ProjectPost" ADD CONSTRAINT "ProjectPost_authorId_fkey" FOREIGN KEY ("authorId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

CREATE TABLE IF NOT EXISTS "ProjectAchievement" (
  "id" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "title" TEXT NOT NULL,
  "description" TEXT,
  "icon" TEXT NOT NULL DEFAULT '🏆',
  "createdById" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProjectAchievement_pkey" PRIMARY KEY ("id")
);
CREATE INDEX IF NOT EXISTS "ProjectAchievement_projectId_createdAt_idx" ON "ProjectAchievement"("projectId", "createdAt");
ALTER TABLE "ProjectAchievement" DROP CONSTRAINT IF EXISTS "ProjectAchievement_projectId_fkey";
ALTER TABLE "ProjectAchievement" ADD CONSTRAINT "ProjectAchievement_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ProjectAchievement" DROP CONSTRAINT IF EXISTS "ProjectAchievement_createdById_fkey";
ALTER TABLE "ProjectAchievement" ADD CONSTRAINT "ProjectAchievement_createdById_fkey" FOREIGN KEY ("createdById") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

CREATE TABLE IF NOT EXISTS "AchievementUnlock" (
  "achievementId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "unlockedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "AchievementUnlock_pkey" PRIMARY KEY ("achievementId", "userId")
);
CREATE INDEX IF NOT EXISTS "AchievementUnlock_userId_idx" ON "AchievementUnlock"("userId");
ALTER TABLE "AchievementUnlock" DROP CONSTRAINT IF EXISTS "AchievementUnlock_achievementId_fkey";
ALTER TABLE "AchievementUnlock" ADD CONSTRAINT "AchievementUnlock_achievementId_fkey" FOREIGN KEY ("achievementId") REFERENCES "ProjectAchievement"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "AchievementUnlock" DROP CONSTRAINT IF EXISTS "AchievementUnlock_userId_fkey";
ALTER TABLE "AchievementUnlock" ADD CONSTRAINT "AchievementUnlock_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

CREATE TABLE IF NOT EXISTS "ProjectReview" (
  "id" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "recommended" BOOLEAN NOT NULL DEFAULT true,
  "body" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProjectReview_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX IF NOT EXISTS "ProjectReview_projectId_userId_key" ON "ProjectReview"("projectId", "userId");
CREATE INDEX IF NOT EXISTS "ProjectReview_projectId_createdAt_idx" ON "ProjectReview"("projectId", "createdAt");
ALTER TABLE "ProjectReview" DROP CONSTRAINT IF EXISTS "ProjectReview_projectId_fkey";
ALTER TABLE "ProjectReview" ADD CONSTRAINT "ProjectReview_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ProjectReview" DROP CONSTRAINT IF EXISTS "ProjectReview_userId_fkey";
ALTER TABLE "ProjectReview" ADD CONSTRAINT "ProjectReview_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
