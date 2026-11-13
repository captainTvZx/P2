CREATE TABLE IF NOT EXISTS "ChannelRead" (
    "channelId" TEXT NOT NULL,
    "userId" TEXT NOT NULL,
    "lastReadAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "ChannelRead_pkey" PRIMARY KEY ("channelId","userId")
);

CREATE INDEX IF NOT EXISTS "ChannelRead_userId_idx" ON "ChannelRead"("userId");

DO $$ BEGIN
  ALTER TABLE "ChannelRead" ADD CONSTRAINT "ChannelRead_channelId_fkey"
  FOREIGN KEY ("channelId") REFERENCES "ProjectChannel"("id") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE "ChannelRead" ADD CONSTRAINT "ChannelRead_userId_fkey"
  FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- Güncelleme anında eski mesajların tamamını birden okunmamış göstermemek için
-- mevcut proje erişimlerini başlangıçta okunmuş kabul et.
INSERT INTO "ChannelRead" ("channelId", "userId", "lastReadAt")
SELECT DISTINCT access."channelId", access."userId", CURRENT_TIMESTAMP
FROM (
  SELECT pc."id" AS "channelId", pm."userId"
  FROM "ProjectChannel" pc
  JOIN "ChannelCategory" cc ON cc."id" = pc."categoryId"
  JOIN "ProjectMember" pm ON pm."projectId" = cc."projectId"
  UNION
  SELECT pc."id", p."ownerId"
  FROM "ProjectChannel" pc
  JOIN "ChannelCategory" cc ON cc."id" = pc."categoryId"
  JOIN "Project" p ON p."id" = cc."projectId"
  UNION
  SELECT pc."id", om."userId"
  FROM "ProjectChannel" pc
  JOIN "ChannelCategory" cc ON cc."id" = pc."categoryId"
  JOIN "Project" p ON p."id" = cc."projectId"
  JOIN "OrganizationMember" om ON om."organizationId" = p."organizationId" AND om."role" = 'OWNER'
) access
ON CONFLICT ("channelId", "userId") DO NOTHING;
