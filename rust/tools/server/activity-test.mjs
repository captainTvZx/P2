// Etkinlik uçları: NestJS ve Rust aynı cevabı veriyor mu (aynı veritabanı).
import { NEST, startRust, call, login, both, done } from './lib.mjs'
const rust = await startRust(4120)
const t = (await login(NEST, 'p2test')).accessToken
const org = (await call(NEST, 'GET', '/organizations', undefined, t)).body[0]
await both('liste', rust.base, 'GET', `/organizations/${org.id}/activity`, undefined, { nest: t })
await both('liste limit=2', rust.base, 'GET', `/organizations/${org.id}/activity?limit=2`, undefined, { nest: t })
await both('liste limit=abc', rust.base, 'GET', `/organizations/${org.id}/activity?limit=abc`, undefined, { nest: t })
await both('yabancı kuruluş', rust.base, 'GET', `/organizations/yok/activity`, undefined, { nest: t })
await both('jetonsuz', rust.base, 'GET', `/organizations/${org.id}/activity`)
await both('tek okundu (olmayan)', rust.base, 'POST', `/organizations/${org.id}/activity/yok/read`, undefined, { nest: t })
rust.stop()
done()
