ALTER TABLE "DirectMessage" ADD COLUMN "quotedMessageId" TEXT;
CREATE INDEX "DirectMessage_quotedMessageId_idx" ON "DirectMessage"("quotedMessageId");
ALTER TABLE "DirectMessage" ADD CONSTRAINT "DirectMessage_quotedMessageId_fkey" FOREIGN KEY ("quotedMessageId") REFERENCES "DirectMessage"("id") ON DELETE SET NULL ON UPDATE CASCADE;
