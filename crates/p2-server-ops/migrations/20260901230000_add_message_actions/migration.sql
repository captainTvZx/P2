ALTER TABLE "ChannelMessage" ADD COLUMN "quotedMessageId" TEXT, ADD COLUMN "forwardedFromId" TEXT;
CREATE TABLE "MessageEditHistory" ("id" TEXT NOT NULL,"messageId" TEXT NOT NULL,"content" TEXT NOT NULL,"createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,CONSTRAINT "MessageEditHistory_pkey" PRIMARY KEY ("id"));
CREATE INDEX "ChannelMessage_quotedMessageId_idx" ON "ChannelMessage"("quotedMessageId");
CREATE INDEX "ChannelMessage_forwardedFromId_idx" ON "ChannelMessage"("forwardedFromId");
CREATE INDEX "MessageEditHistory_messageId_createdAt_idx" ON "MessageEditHistory"("messageId","createdAt");
ALTER TABLE "ChannelMessage" ADD CONSTRAINT "ChannelMessage_quotedMessageId_fkey" FOREIGN KEY ("quotedMessageId") REFERENCES "ChannelMessage"("id") ON DELETE SET NULL ON UPDATE CASCADE;
ALTER TABLE "ChannelMessage" ADD CONSTRAINT "ChannelMessage_forwardedFromId_fkey" FOREIGN KEY ("forwardedFromId") REFERENCES "ChannelMessage"("id") ON DELETE SET NULL ON UPDATE CASCADE;
ALTER TABLE "MessageEditHistory" ADD CONSTRAINT "MessageEditHistory_messageId_fkey" FOREIGN KEY ("messageId") REFERENCES "ChannelMessage"("id") ON DELETE CASCADE ON UPDATE CASCADE;
