ALTER TABLE "ChatGroupMessage" ADD COLUMN "quotedMessageId" TEXT;
CREATE INDEX "ChatGroupMessage_quotedMessageId_idx" ON "ChatGroupMessage"("quotedMessageId");
ALTER TABLE "ChatGroupMessage" ADD CONSTRAINT "ChatGroupMessage_quotedMessageId_fkey" FOREIGN KEY ("quotedMessageId") REFERENCES "ChatGroupMessage"("id") ON DELETE SET NULL ON UPDATE CASCADE;

CREATE TABLE "ChatReaction" (
  "id" TEXT NOT NULL,
  "directMessageId" TEXT,
  "groupMessageId" TEXT,
  "userId" TEXT NOT NULL,
  "emoji" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ChatReaction_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "ChatReaction_directMessageId_userId_emoji_key" ON "ChatReaction"("directMessageId", "userId", "emoji");
CREATE UNIQUE INDEX "ChatReaction_groupMessageId_userId_emoji_key" ON "ChatReaction"("groupMessageId", "userId", "emoji");
CREATE INDEX "ChatReaction_directMessageId_idx" ON "ChatReaction"("directMessageId");
CREATE INDEX "ChatReaction_groupMessageId_idx" ON "ChatReaction"("groupMessageId");
ALTER TABLE "ChatReaction" ADD CONSTRAINT "ChatReaction_directMessageId_fkey" FOREIGN KEY ("directMessageId") REFERENCES "DirectMessage"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ChatReaction" ADD CONSTRAINT "ChatReaction_groupMessageId_fkey" FOREIGN KEY ("groupMessageId") REFERENCES "ChatGroupMessage"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "ChatReaction" ADD CONSTRAINT "ChatReaction_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
