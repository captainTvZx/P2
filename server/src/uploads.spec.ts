import { describe, expect, it } from 'vitest'
import { rangeOf, sniffType } from './uploads'

describe('upload tür tespiti',()=>{
  it('istemcinin MIME değerinden önce imza ve uzantıyı kullanır',()=>{
    expect(sniffType(Buffer.from('%PDF-1.7'),'yanlış.bin','text/plain')).toBe('application/pdf')
    expect(sniffType(Buffer.from('ID3\x04\0\0'),'ses.bin','application/octet-stream')).toBe('audio/mpeg')
    expect(sniffType(Buffer.from('<svg/>'),'çizim.svg','application/octet-stream')).toBe('image/svg+xml')
  })
})

describe('HTTP byte range',()=>{
  it('başlangıç, açık uç ve suffix aralıklarını çözer',()=>{
    expect(rangeOf('bytes=3-8',20)).toEqual({start:3,end:8})
    expect(rangeOf('bytes=5-',20)).toEqual({start:5,end:19})
    expect(rangeOf('bytes=-4',20)).toEqual({start:16,end:19})
  })
  it('geçersiz aralıkları reddeder',()=>{
    expect(rangeOf('bytes=30-40',20)).toBe(false)
    expect(rangeOf('items=0-2',20)).toBe(false)
  })
})
