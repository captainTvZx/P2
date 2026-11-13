ALTER TABLE "ProjectChannel" ADD COLUMN "deletedAt" TIMESTAMP(3), ADD COLUMN "purgeAt" TIMESTAMP(3);

CREATE TABLE "ChannelMessage" (
  "id" TEXT NOT NULL,
  "content" TEXT NOT NULL,
  "channelId" TEXT NOT NULL,
  "authorId" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "editedAt" TIMESTAMP(3),
  CONSTRAINT "ChannelMessage_pkey" PRIMARY KEY ("id")
);

CREATE INDEX "ProjectChannel_purgeAt_idx" ON "ProjectChannel"("purgeAt");
CREATE INDEX "ChannelMessage_channelId_createdAt_idx" ON "ChannelMessage"("channelId", "createdAt");
ALTER TABLE "ChannelMessage" ADD CONSTRAINT "ChannelMessage_channelId_fkey" FOREIGN KEY ("channelId") REFERENCES "ProjectChannel"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ChannelMessage" ADD CONSTRAINT "ChannelMessage_authorId_fkey" FOREIGN KEY ("authorId") REFERENCES "User"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
