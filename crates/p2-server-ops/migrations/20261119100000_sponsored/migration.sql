-- Sponsorlu içerik (uygulama içi reklam): sunucu yöneticisinin yayınladığı sade kartlar. Keşfet'te,
-- Keşfet haber sütununda ve Kütüphane kenar çubuğunda en çok birer tane görünür; açılır pencere, ses,
-- video ya da kişisel izleme yoktur. Gösterim ve tıklama yalnız gün başına toplam sayılır (kişi başına
-- günde bir kez; kim olduğu saklanmaz).
-- Yalnız ekler; tekrar çalıştırılsa da aynı sonucu verir.

CREATE TABLE IF NOT EXISTS "SponsoredAd" (
    "id" TEXT NOT NULL,
    "sponsor" TEXT NOT NULL,
    "title" TEXT NOT NULL,
    "body" TEXT NOT NULL DEFAULT '',
    "imageUrl" TEXT,
    "linkUrl" TEXT NOT NULL,
    "cta" TEXT NOT NULL DEFAULT '',
    -- 'discover' | 'news' | 'library'
    "placements" TEXT[] NOT NULL DEFAULT '{}',
    "active" BOOLEAN NOT NULL DEFAULT true,
    "weight" INTEGER NOT NULL DEFAULT 1,
    "startsAt" TIMESTAMP(3),
    "endsAt" TIMESTAMP(3),
    "createdById" TEXT,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "SponsoredAd_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "SponsoredAd_createdById_fkey" FOREIGN KEY ("createdById") REFERENCES "User"("id") ON DELETE SET NULL ON UPDATE CASCADE
);

-- Günlük toplamlar (raporlama ve sponsora fatura için).
CREATE TABLE IF NOT EXISTS "SponsoredAdStat" (
    "adId" TEXT NOT NULL,
    "day" DATE NOT NULL,
    "impressions" BIGINT NOT NULL DEFAULT 0,
    "clicks" BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT "SponsoredAdStat_pkey" PRIMARY KEY ("adId", "day"),
    CONSTRAINT "SponsoredAdStat_adId_fkey" FOREIGN KEY ("adId") REFERENCES "SponsoredAd"("id") ON DELETE CASCADE ON UPDATE CASCADE
);
