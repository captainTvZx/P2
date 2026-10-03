ALTER TABLE "User"
ADD COLUMN "lastClientVersion" TEXT,
ADD COLUMN "lastClientSeenAt" TIMESTAMP(3);
