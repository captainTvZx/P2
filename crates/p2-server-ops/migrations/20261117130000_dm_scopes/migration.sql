-- Birebir sohbet kapsamları (docs/arkadaslar.md): Ana sayfa sohbeti ile proje sohbeti ayrıdır.
--  * "DirectConversation"."projectId": NULL = Ana sayfa (arkadaşlar / hesaba bağlı), dolu = o projenin
--    içindeki sohbet. Aynı iki kişinin bir Ana sayfa sohbeti ve paylaştıkları her proje için birer
--    sohbeti olabilir. Var olan bütün sohbetler Ana sayfa sohbeti olarak kalır ("projectId" NULL).
--  * "userAHidden" / "userBHidden": kişi sohbeti kendi listesinden kaldırdı (mesajlar silinmez; yeni
--    mesajda ya da sohbet yeniden açılınca geri gelir).
--  * "ChatGroup"."home": projeye bağlı olmayan Ana sayfa grubu ("projectId" NULL). Ayrı sütun, eski
--    yetim (projesiz) gruplar kendiliğinden Ana sayfada belirmesin diye.
--  * "UserPreference"."scope": yıldızlar kapsamlıdır. Kanal tercihlerinde '' (kanal zaten projeye ait),
--    kişi tercihlerinde 'home' ya da proje kimliği.
-- Göç yalnız ekler ve tekrar çalıştırılabilir.

ALTER TABLE "DirectConversation" ADD COLUMN IF NOT EXISTS "projectId" TEXT;
ALTER TABLE "DirectConversation" ADD COLUMN IF NOT EXISTS "userAHidden" BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE "DirectConversation" ADD COLUMN IF NOT EXISTS "userBHidden" BOOLEAN NOT NULL DEFAULT false;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'DirectConversation_projectId_fkey') THEN
        ALTER TABLE "DirectConversation" ADD CONSTRAINT "DirectConversation_projectId_fkey"
            FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
    END IF;
END $$;

-- Çift anahtarı artık kapsamı da içerir: Ana sayfada kuruluş başına tek, projede proje başına tek sohbet.
DROP INDEX IF EXISTS "DirectConversation_organizationId_userAId_userBId_key";
CREATE UNIQUE INDEX IF NOT EXISTS "DirectConversation_home_pair_key"
    ON "DirectConversation"("organizationId", "userAId", "userBId") WHERE "projectId" IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS "DirectConversation_project_pair_key"
    ON "DirectConversation"("projectId", "userAId", "userBId") WHERE "projectId" IS NOT NULL;
CREATE INDEX IF NOT EXISTS "DirectConversation_projectId_updatedAt_idx" ON "DirectConversation"("projectId", "updatedAt");

ALTER TABLE "ChatGroup" ADD COLUMN IF NOT EXISTS "home" BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE "UserPreference" ADD COLUMN IF NOT EXISTS "scope" TEXT NOT NULL DEFAULT '';
ALTER TABLE "UserPreference" DROP CONSTRAINT IF EXISTS "UserPreference_pkey";
ALTER TABLE "UserPreference" ADD CONSTRAINT "UserPreference_pkey" PRIMARY KEY ("userId", "kind", "targetId", "scope");

-- Eski kişi tercihleri kapsamsızdı ve her projenin kenar çubuğunda görünüyordu: iki kişinin paylaştığı
-- her projeye kopyalanır (proje kenar çubukları olduğu gibi kalır); ortak proje yoksa Ana sayfaya taşınır.
INSERT INTO "UserPreference" ("userId", "organizationId", "kind", "targetId", "scope", "starred", "notify", "hidden", "updatedAt")
SELECT up."userId", up."organizationId", up."kind", up."targetId", p."id", up."starred", up."notify", up."hidden", up."updatedAt"
FROM "UserPreference" up
JOIN "ProjectMember" mine ON mine."userId" = up."userId"
JOIN "ProjectMember" theirs ON theirs."projectId" = mine."projectId" AND theirs."userId" = up."targetId"
JOIN "Project" p ON p."id" = mine."projectId" AND p."deletedAt" IS NULL AND p."organizationId" = up."organizationId"
WHERE up."kind" = 'PERSON' AND up."scope" = ''
ON CONFLICT DO NOTHING;

UPDATE "UserPreference" up SET "scope" = 'home'
WHERE up."kind" = 'PERSON' AND up."scope" = ''
  AND NOT EXISTS (SELECT 1 FROM "UserPreference" x WHERE x."userId" = up."userId" AND x."kind" = 'PERSON' AND x."targetId" = up."targetId" AND x."scope" <> '');

DELETE FROM "UserPreference" WHERE "kind" = 'PERSON' AND "scope" = '';
