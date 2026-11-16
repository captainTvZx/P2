-- Arkadaşlık sistemi: arkadaşlıklar hesaba bağlıdır (projeden bağımsız), engeller ayrı tabloda tutulur.
-- "Friendship" (init göçünden beri var, hiç yazılmadı) artık yalnız PENDING / ACCEPTED taşır;
-- BLOCKED satırları yönlü "UserBlock" tablosuna taşınır. Göç yalnız ekler ve tekrar çalıştırılabilir.

CREATE TABLE IF NOT EXISTS "UserBlock" (
    "blockerId" TEXT NOT NULL,
    "blockedId" TEXT NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "UserBlock_pkey" PRIMARY KEY ("blockerId", "blockedId"),
    CONSTRAINT "UserBlock_blockerId_fkey" FOREIGN KEY ("blockerId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT "UserBlock_blockedId_fkey" FOREIGN KEY ("blockedId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE INDEX IF NOT EXISTS "UserBlock_blockedId_idx" ON "UserBlock"("blockedId");

-- Eski BLOCKED satırları: isteği gönderen engelleyen sayılır.
INSERT INTO "UserBlock" ("blockerId", "blockedId", "createdAt")
SELECT f."requesterId", f."addresseeId", f."createdAt" FROM "Friendship" f
WHERE f."status"::text = 'BLOCKED' AND f."requesterId" <> f."addresseeId"
ON CONFLICT DO NOTHING;
DELETE FROM "Friendship" WHERE "status"::text = 'BLOCKED' OR "requesterId" = "addresseeId";

-- Bir çift için tek satır: ters yönlü kopyalardan kabul edilmiş olan (yoksa eskisi) kalır.
DELETE FROM "Friendship" f USING "Friendship" g
WHERE f."requesterId" = g."addresseeId" AND f."addresseeId" = g."requesterId" AND f."id" <> g."id"
  AND ((f."status"::text <> 'ACCEPTED' AND g."status"::text = 'ACCEPTED')
    OR (f."status"::text = g."status"::text AND (f."createdAt", f."id") > (g."createdAt", g."id")));

ALTER TABLE "Friendship" ADD COLUMN IF NOT EXISTS "acceptedAt" TIMESTAMP(3);
UPDATE "Friendship" SET "acceptedAt" = "createdAt" WHERE "status"::text = 'ACCEPTED' AND "acceptedAt" IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS "Friendship_pair_key" ON "Friendship"(LEAST("requesterId", "addresseeId"), GREATEST("requesterId", "addresseeId"));
CREATE INDEX IF NOT EXISTS "Friendship_addresseeId_idx" ON "Friendship"("addresseeId");
