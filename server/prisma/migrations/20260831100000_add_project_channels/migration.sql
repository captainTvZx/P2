CREATE TYPE "ProjectChannelType" AS ENUM ('TEXT', 'ANNOUNCEMENT', 'VOICE', 'FORUM');
CREATE TABLE "ChannelCategory" (
  "id" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "position" INTEGER NOT NULL DEFAULT 0,
  "projectId" TEXT NOT NULL,
  CONSTRAINT "ChannelCategory_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "ProjectChannel" (
  "id" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "type" "ProjectChannelType" NOT NULL DEFAULT 'TEXT',
  "position" INTEGER NOT NULL DEFAULT 0,
  "categoryId" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProjectChannel_pkey" PRIMARY KEY ("id")
);
CREATE INDEX "ChannelCategory_projectId_position_idx" ON "ChannelCategory"("projectId", "position");
CREATE INDEX "ProjectChannel_categoryId_position_idx" ON "ProjectChannel"("categoryId", "position");
ALTER TABLE "ChannelCategory" ADD CONSTRAINT "ChannelCategory_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ProjectChannel" ADD CONSTRAINT "ProjectChannel_categoryId_fkey" FOREIGN KEY ("categoryId") REFERENCES "ChannelCategory"("id") ON DELETE CASCADE ON UPDATE CASCADE;
