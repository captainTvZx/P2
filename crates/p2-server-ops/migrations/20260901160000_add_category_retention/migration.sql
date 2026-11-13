ALTER TABLE "ChannelCategory" ADD COLUMN "deletedAt" TIMESTAMP(3), ADD COLUMN "purgeAt" TIMESTAMP(3);
CREATE INDEX "ChannelCategory_purgeAt_idx" ON "ChannelCategory"("purgeAt");
