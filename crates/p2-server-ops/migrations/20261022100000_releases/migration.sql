-- Kütüphane: içerik adresli (4 MB, sha256) parçalardan oluşan oyun sürümleri.
CREATE TABLE IF NOT EXISTS "Release" (
  "id" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "channel" TEXT NOT NULL,
  "version" TEXT NOT NULL,
  "notes" TEXT,
  "status" TEXT NOT NULL DEFAULT 'DRAFT',
  "executable" TEXT NOT NULL,
  "launchArgs" TEXT,
  "platform" TEXT NOT NULL DEFAULT 'win-x64',
  "overlay" BOOLEAN NOT NULL DEFAULT true,
  "totalSize" BIGINT NOT NULL DEFAULT 0,
  "fileCount" INTEGER NOT NULL DEFAULT 0,
  "chunkCount" INTEGER NOT NULL DEFAULT 0,
  "manifest" JSONB NOT NULL DEFAULT '{}',
  "createdById" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "publishedAt" TIMESTAMP(3),
  "archivedAt" TIMESTAMP(3),
  CONSTRAINT "Release_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX IF NOT EXISTS "Release_projectId_channel_version_key" ON "Release"("projectId", "channel", "version");
CREATE INDEX IF NOT EXISTS "Release_projectId_status_publishedAt_idx" ON "Release"("projectId", "status", "publishedAt");

-- Aynı özetli parça bütün sürümler arasında bir kez saklanır.
CREATE TABLE IF NOT EXISTS "ReleaseChunk" (
  "hash" TEXT NOT NULL,
  "size" INTEGER NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ReleaseChunk_pkey" PRIMARY KEY ("hash")
);

-- Sürüm -> parça bağlantısı; referans sayısı buradan türetilir, GC ref'siz parçayı siler.
CREATE TABLE IF NOT EXISTS "ReleaseChunkRef" (
  "releaseId" TEXT NOT NULL,
  "hash" TEXT NOT NULL,
  CONSTRAINT "ReleaseChunkRef_pkey" PRIMARY KEY ("releaseId", "hash")
);
CREATE INDEX IF NOT EXISTS "ReleaseChunkRef_hash_idx" ON "ReleaseChunkRef"("hash");

-- Hangi cihazda hangi sürüm kurulu.
CREATE TABLE IF NOT EXISTS "ReleaseInstall" (
  "id" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "releaseId" TEXT,
  "userId" TEXT NOT NULL,
  "deviceId" TEXT NOT NULL,
  "channel" TEXT NOT NULL,
  "version" TEXT NOT NULL,
  "status" TEXT NOT NULL DEFAULT 'INSTALLED',
  "pinned" BOOLEAN NOT NULL DEFAULT false,
  "clientVersion" TEXT,
  "installedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "lastLaunchedAt" TIMESTAMP(3),
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ReleaseInstall_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX IF NOT EXISTS "ReleaseInstall_projectId_userId_deviceId_key" ON "ReleaseInstall"("projectId", "userId", "deviceId");
CREATE INDEX IF NOT EXISTS "ReleaseInstall_releaseId_idx" ON "ReleaseInstall"("releaseId");

DO $$ BEGIN
  ALTER TABLE "Release" ADD CONSTRAINT "Release_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN
  ALTER TABLE "Release" ADD CONSTRAINT "Release_createdById_fkey" FOREIGN KEY ("createdById") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN
  ALTER TABLE "ReleaseChunkRef" ADD CONSTRAINT "ReleaseChunkRef_releaseId_fkey" FOREIGN KEY ("releaseId") REFERENCES "Release"("id") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN
  ALTER TABLE "ReleaseChunkRef" ADD CONSTRAINT "ReleaseChunkRef_hash_fkey" FOREIGN KEY ("hash") REFERENCES "ReleaseChunk"("hash") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN
  ALTER TABLE "ReleaseInstall" ADD CONSTRAINT "ReleaseInstall_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN
  ALTER TABLE "ReleaseInstall" ADD CONSTRAINT "ReleaseInstall_releaseId_fkey" FOREIGN KEY ("releaseId") REFERENCES "Release"("id") ON DELETE SET NULL ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN
  ALTER TABLE "ReleaseInstall" ADD CONSTRAINT "ReleaseInstall_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- Proje kapak görseli ve ikonu (kütüphane vitrini / ray avatarı).
ALTER TABLE "Project" ADD COLUMN IF NOT EXISTS "coverUrl" TEXT;
ALTER TABLE "Project" ADD COLUMN IF NOT EXISTS "iconUrl" TEXT;

-- Mevcut kuruluşlarda herkes kütüphaneden indirebilsin (yeni yetki kataloğa eklendi).
UPDATE "Role" SET "permissions" = array_append("permissions", 'release.download')
WHERE "everyone" = true AND NOT ('release.download' = ANY("permissions"));
