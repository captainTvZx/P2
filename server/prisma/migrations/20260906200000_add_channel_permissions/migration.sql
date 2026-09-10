CREATE TABLE "ChannelPermission" (
  "id" TEXT NOT NULL,
  "channelId" TEXT NOT NULL,
  "targetType" TEXT NOT NULL,
  "targetId" TEXT NOT NULL,
  "allow" TEXT[] NOT NULL DEFAULT '{}',
  "deny" TEXT[] NOT NULL DEFAULT '{}',
  "updatedAt" TIMESTAMP(3) NOT NULL,
  CONSTRAINT "ChannelPermission_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "ChannelPermission_channelId_targetType_targetId_key" ON "ChannelPermission"("channelId", "targetType", "targetId");
CREATE INDEX "ChannelPermission_channelId_idx" ON "ChannelPermission"("channelId");
ALTER TABLE "ChannelPermission" ADD CONSTRAINT "ChannelPermission_channelId_fkey" FOREIGN KEY ("channelId") REFERENCES "ProjectChannel"("id") ON DELETE CASCADE ON UPDATE CASCADE;
