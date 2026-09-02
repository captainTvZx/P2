CREATE TABLE "ChatGroup" (
  "id" TEXT NOT NULL,
  "organizationId" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "createdById" TEXT NOT NULL,
  "channelId" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL,
  CONSTRAINT "ChatGroup_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "ChatGroupMember" (
  "id" TEXT NOT NULL,
  "groupId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "lastReadAt" TIMESTAMP(3),
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ChatGroupMember_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "ChatGroupMessage" (
  "id" TEXT NOT NULL,
  "groupId" TEXT NOT NULL,
  "authorId" TEXT NOT NULL,
  "content" TEXT NOT NULL,
  "kind" TEXT NOT NULL DEFAULT 'TEXT',
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "editedAt" TIMESTAMP(3),
  CONSTRAINT "ChatGroupMessage_pkey" PRIMARY KEY ("id")
);
CREATE INDEX "ChatGroup_organizationId_updatedAt_idx" ON "ChatGroup"("organizationId", "updatedAt");
CREATE UNIQUE INDEX "ChatGroupMember_groupId_userId_key" ON "ChatGroupMember"("groupId", "userId");
CREATE INDEX "ChatGroupMember_userId_idx" ON "ChatGroupMember"("userId");
CREATE INDEX "ChatGroupMessage_groupId_createdAt_idx" ON "ChatGroupMessage"("groupId", "createdAt");
ALTER TABLE "ChatGroup" ADD CONSTRAINT "ChatGroup_organizationId_fkey" FOREIGN KEY ("organizationId") REFERENCES "Organization"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ChatGroupMember" ADD CONSTRAINT "ChatGroupMember_groupId_fkey" FOREIGN KEY ("groupId") REFERENCES "ChatGroup"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ChatGroupMember" ADD CONSTRAINT "ChatGroupMember_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ChatGroupMessage" ADD CONSTRAINT "ChatGroupMessage_groupId_fkey" FOREIGN KEY ("groupId") REFERENCES "ChatGroup"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ChatGroupMessage" ADD CONSTRAINT "ChatGroupMessage_authorId_fkey" FOREIGN KEY ("authorId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
