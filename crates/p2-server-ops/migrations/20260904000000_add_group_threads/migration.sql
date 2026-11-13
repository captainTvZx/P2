ALTER TABLE "ChatGroupMessage" ADD COLUMN "parentId" TEXT;
CREATE INDEX "ChatGroupMessage_parentId_createdAt_idx" ON "ChatGroupMessage"("parentId", "createdAt");
ALTER TABLE "ChatGroupMessage" ADD CONSTRAINT "ChatGroupMessage_parentId_fkey" FOREIGN KEY ("parentId") REFERENCES "ChatGroupMessage"("id") ON DELETE CASCADE ON UPDATE CASCADE;
