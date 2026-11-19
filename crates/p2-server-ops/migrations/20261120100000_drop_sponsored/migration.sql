-- Sponsorlu içerik özelliği kaldırıldı (1.0.2): tablolar, ayar ve yetki temizlenir.
DROP TABLE IF EXISTS "SponsoredAdStat";
DROP TABLE IF EXISTS "SponsoredAd";
DELETE FROM "InstanceSetting" WHERE key = 'sponsoredEnabled';
DELETE FROM "ServerPermission" WHERE permission = 'server.ads';
