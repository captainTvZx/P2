ALTER TABLE "ChannelMessage" ADD COLUMN "parentId" TEXT;
CREATE INDEX "ChannelMessage_parentId_createdAt_idx" ON "ChannelMessage"("parentId", "createdAt");
ALTER TABLE "ChannelMessage" ADD CONSTRAINT "ChannelMessage_parentId_fkey" FOREIGN KEY ("parentId") REFERENCES "ChannelMessage"("id") ON DELETE CASCADE ON UPDATE CASCADE;
