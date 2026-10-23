import { describe, expect, it } from 'vitest'
import { BUG_DESCRIPTION_LIMIT, buildBugChannelMessage, buildBugDescription, formatPlaySeconds, pickReportChannel } from './bugReport'

const marker='[[p2-file|shot.jpg|image%2Fjpeg|1234|/uploads/3f2a1c9e-1b2c-4d5e-8f90-123456789abc.jpg]]'

describe('formatPlaySeconds', () => {
  it('dakika:saniye, saat varsa başa', () => {
    expect(formatPlaySeconds(65)).toBe('01:05')
    expect(formatPlaySeconds(3725)).toBe('1:02:05')
    expect(formatPlaySeconds(0)).toBe('00:00')
    expect(formatPlaySeconds(-5)).toBe('00:00')
  })
})

describe('buildBugDescription', () => {
  it('başlık bloğu, sürüm satırı, ayırıcı, açıklama ve ek işareti sırasıyla', () => {
    const text=buildBugDescription({projectName:'MineTastic',version:'1.4.2',channel:'beta',releaseId:'rel-1',clientVersion:'0.8.2',playSeconds:125,description:'Kapı açılmıyor.',attachment:marker})
    expect(text.split('\n')).toEqual([
      '**Hata bildirimi** (oyun içi overlay)',
      'Proje: MineTastic',
      'Sürüm: 1.4.2 (beta) · Sürüm kimliği: rel-1',
      'İstemci: P2 0.8.2',
      'İşletim sistemi: Windows',
      'Oynama süresi (bu oturum): 02:05',
      '---',
      'Kapı açılmıyor.',
      marker,
    ])
  })
  it('sürüm bilinmiyorsa ve oynama süresi yoksa satırlar düşer', () => {
    const text=buildBugDescription({projectName:'X',description:''})
    expect(text).toContain('Sürüm: kurulu sürüm bilinmiyor')
    expect(text).toContain('İstemci: P2 ?')
    expect(text).not.toContain('Oynama süresi')
    expect(text.endsWith('---')).toBe(true)
  })
  it('ek olmadan işaret satırı eklenmez', () => {
    expect(buildBugDescription({projectName:'X',description:'a'})).not.toContain('[[p2-file')
  })
  it('uzun açıklama kırpılır, ek işareti korunur', () => {
    const text=buildBugDescription({projectName:'X',description:'y'.repeat(BUG_DESCRIPTION_LIMIT+500),attachment:marker})
    expect(text.length).toBeLessThanOrEqual(BUG_DESCRIPTION_LIMIT)
    expect(text.endsWith('\n'+marker)).toBe(true)
  })
})

describe('pickReportChannel', () => {
  const categories=[
    {channels:[{id:'a',name:'başlangıç',type:'ANNOUNCEMENT'},{id:'b',name:'geliştirme',type:'TEXT'}]},
    {channels:[{id:'c',name:'Genel',type:'TEXT'},{id:'d',name:'ekip-odası',type:'VOICE'}]},
  ]
  it("'genel' adlı TEXT kanal önceliklidir (büyük/küçük harf duyarsız)", () => {
    expect(pickReportChannel(categories)?.id).toBe('c')
  })
  it("'genel' yoksa ilk TEXT kanal, hiç yoksa null", () => {
    expect(pickReportChannel([categories[0]])?.id).toBe('b')
    expect(pickReportChannel([{channels:[{id:'v',name:'ses',type:'VOICE'}]}])).toBeNull()
  })
})

describe('buildBugChannelMessage', () => {
  it('başlık, önem, görev numarası ve ek', () => {
    expect(buildBugChannelMessage({title:' Kapı ',severityLabel:'Yüksek',taskNumber:12,attachment:marker})).toBe(`**Hata bildirimi:** Kapı · Önem: Yüksek · Görev #12\n${marker}`)
    expect(buildBugChannelMessage({title:'Kapı',severityLabel:'Düşük'})).toBe('**Hata bildirimi:** Kapı · Önem: Düşük')
  })
})
