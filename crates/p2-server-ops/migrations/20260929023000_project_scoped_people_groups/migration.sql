ALTER TABLE "ChatGroup" ADD COLUMN "projectId" TEXT;

-- Kanal aramasından oluşan gruplar doğrudan kanalın projesine bağlanır.
UPDATE "ChatGroup" AS chat_group
SET "projectId" = category."projectId"
FROM "ProjectChannel" AS channel
JOIN "ChannelCategory" AS category ON category."id" = channel."categoryId"
WHERE chat_group."channelId" = channel."id";

-- Eski elle oluşturulmuş grupları, kurucunun erişebildiği en güncel projeye
-- bağlayarak konuşmaları kaybetmeden proje kapsamına geçir.
UPDATE "ChatGroup" AS chat_group
SET "projectId" = (
  SELECT member."projectId"
  FROM "ProjectMember" AS member
  JOIN "Project" AS project ON project."id" = member."projectId"
  WHERE member."userId" = chat_group."createdById"
    AND project."organizationId" = chat_group."organizationId"
    AND project."deletedAt" IS NULL
  ORDER BY project."updatedAt" DESC
  LIMIT 1
)
WHERE chat_group."projectId" IS NULL;

ALTER TABLE "ChatGroup"
ADD CONSTRAINT "ChatGroup_projectId_fkey"
FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;

CREATE INDEX "ChatGroup_projectId_updatedAt_idx" ON "ChatGroup"("projectId", "updatedAt");
