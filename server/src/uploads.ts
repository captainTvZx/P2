import { BadRequestException, Controller, Get, Head, Param, Post, Req, Res, UseGuards } from '@nestjs/common'
import { randomUUID } from 'node:crypto'
import { createReadStream } from 'node:fs'
import { mkdir, open, readFile, stat, writeFile } from 'node:fs/promises'
import { extname, join } from 'node:path'
import { AuthGuard } from './auth'

const root=process.env.P2_UPLOAD_DIR||join(process.cwd(),'.p2-data','uploads')
const safeName=(value:string)=>value.replace(/[\x00-\x1f\\/:*?"<>|]/g,'_').slice(0,180)||'dosya'
const allowedExt=(name:string)=>extname(name).replace(/[^a-zA-Z0-9.]/g,'').slice(0,12)
const mimeByExt:Record<string,string>={
  '.svg':'image/svg+xml','.png':'image/png','.jpg':'image/jpeg','.jpeg':'image/jpeg','.gif':'image/gif','.webp':'image/webp','.avif':'image/avif','.bmp':'image/bmp','.ico':'image/x-icon',
  '.mp3':'audio/mpeg','.wav':'audio/wav','.ogg':'audio/ogg','.oga':'audio/ogg','.opus':'audio/ogg','.m4a':'audio/mp4','.aac':'audio/aac','.flac':'audio/flac',
  '.mp4':'video/mp4','.m4v':'video/mp4','.webm':'video/webm','.ogv':'video/ogg','.mov':'video/quicktime',
  '.pdf':'application/pdf','.json':'application/json','.xml':'application/xml','.csv':'text/csv','.tsv':'text/tab-separated-values','.md':'text/markdown','.txt':'text/plain','.log':'text/plain','.yaml':'text/yaml','.yml':'text/yaml',
  '.docx':'application/vnd.openxmlformats-officedocument.wordprocessingml.document','.xlsx':'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet','.xls':'application/vnd.ms-excel','.ods':'application/vnd.oasis.opendocument.spreadsheet',
  '.pptx':'application/vnd.openxmlformats-officedocument.presentationml.presentation','.odp':'application/vnd.oasis.opendocument.presentation','.odt':'application/vnd.oasis.opendocument.text',
  '.zip':'application/zip','.rar':'application/vnd.rar','.7z':'application/x-7z-compressed','.tar':'application/x-tar','.gz':'application/gzip','.epub':'application/epub+zip',
  '.ttf':'font/ttf','.otf':'font/otf','.woff':'font/woff','.woff2':'font/woff2',
}
const sourceExt=/\.(?:[cm]?[jt]sx?|tsx?|py|rb|rs|go|java|kt|kts|cs|cpp|cxx|cc|c|h|hpp|php|swift|sql|sh|ps1|bat|ini|toml|conf|properties|gradle|vue|svelte|css|scss|sass|less|html?)$/i
export function sniffType(buffer:Buffer,name:string,declared:string){
  if(buffer.subarray(0,8).equals(Buffer.from([0x89,0x50,0x4e,0x47,0x0d,0x0a,0x1a,0x0a])))return'image/png'
  if(buffer[0]===0xff&&buffer[1]===0xd8&&buffer[2]===0xff)return'image/jpeg'
  if(buffer.subarray(0,4).toString()==='%PDF')return'application/pdf'
  if(buffer.subarray(0,4).toString()==='OggS')return declared.startsWith('video/')?'video/ogg':'audio/ogg'
  if(buffer.subarray(0,4).toString()==='fLaC')return'audio/flac'
  if(buffer.subarray(0,3).toString()==='ID3'||(buffer[0]===0xff&&(buffer[1]&0xe0)===0xe0))return'audio/mpeg'
  if(buffer.subarray(0,4).toString()==='RIFF')return buffer.subarray(8,12).toString()==='WAVE'?'audio/wav':declared
  if(buffer.subarray(4,8).toString()==='ftyp')return /\.(?:m4a|aac)$/i.test(name)?'audio/mp4':/\.mov$/i.test(name)?'video/quicktime':'video/mp4'
  const extension=extname(name).toLowerCase(),mapped=mimeByExt[extension]
  if(mapped)return mapped
  if(sourceExt.test(name))return'text/plain'
  const sample=buffer.subarray(0,4096)
  if(sample.length&&!sample.includes(0)&&sample.toString('utf8').replace(/[\t\n\r\x20-\x7e\u00a0-\uffff]/g,'').length<8)return declared.startsWith('text/')?declared:'text/plain'
  return'application/octet-stream'
}
const inlineType=(type:string)=>/^(?:image\/(?:png|jpeg|gif|webp|avif|bmp|x-icon)|audio\/|video\/|application\/pdf|text\/|application\/(?:json|xml))/.test(type)
export function rangeOf(value:string|undefined,size:number){
  if(!value)return null
  const match=/^bytes=(\d*)-(\d*)$/.exec(value.trim())
  if(!match)return false
  let start=match[1]?Number(match[1]):NaN,end=match[2]?Number(match[2]):NaN
  if(Number.isNaN(start)){const suffix=end;if(!Number.isFinite(suffix)||suffix<=0)return false;start=Math.max(0,size-suffix);end=size-1}
  else {if(!Number.isFinite(end))end=size-1;if(start<0||start>=size||end<start)return false;end=Math.min(end,size-1)}
  return {start,end}
}

@Controller('uploads')
export class UploadsController {
  @UseGuards(AuthGuard)
  @Post()
  async upload(@Req() req:any){
    const name=safeName(String(req.body?.name||'dosya'))
    const declared=String(req.body?.type||'application/octet-stream').slice(0,120)
    const encoded=String(req.body?.data||'')
    const match=/^data:[^;]*;base64,(.+)$/.exec(encoded)
    if(!match)throw new BadRequestException({code:'INVALID_FILE',message:'Dosya verisi geçersiz.'})
    const buffer=Buffer.from(match[1],'base64')
    if(!buffer.length||buffer.length>250*1024*1024)throw new BadRequestException({code:'FILE_SIZE',message:'Dosya 250 MB sınırını aşıyor.'})
    const type=sniffType(buffer,name,declared)
    await mkdir(root,{recursive:true})
    const id=randomUUID()+allowedExt(name)
    await writeFile(join(root,id),buffer,{flag:'wx'})
    const meta={name,type,declaredType:declared,size:buffer.length,userId:req.user.sub}
    await writeFile(join(root,`${id}.json`),JSON.stringify(meta),'utf8')
    return {name,type,size:buffer.length,url:`/uploads/${id}`}
  }

  @Head(':id')
  async head(@Param('id') id:string,@Req() request:any,@Res() response:any){return this.serve(id,request,response,true)}

  @Get(':id')
  async file(@Param('id') id:string,@Req() request:any,@Res() response:any){return this.serve(id,request,response,false)}

  private async serve(id:string,request:any,response:any,headOnly:boolean){
    if(!/^[a-f0-9-]{36}(\.[a-zA-Z0-9]{1,10})?$/.test(id))return response.status(404).end()
    try{
      const path=join(root,id),[info,raw]=await Promise.all([stat(path),readFile(`${path}.json`,'utf8')])
      const meta=JSON.parse(raw),handle=await open(path,'r'),sample=Buffer.alloc(Math.min(8192,info.size))
      await handle.read(sample,0,sample.length,0);await handle.close()
      const type=sniffType(sample,String(meta.name||id),String(meta.type||meta.declaredType||''))
      const range=rangeOf(request.headers.range,info.size)
      response.setHeader('Accept-Ranges','bytes')
      response.setHeader('Content-Type',type.startsWith('text/')?`${type}; charset=utf-8`:type)
      response.setHeader('Content-Disposition',`${inlineType(type)?'inline':'attachment'}; filename*=UTF-8''${encodeURIComponent(meta.name||'file')}`)
      response.setHeader('X-Content-Type-Options','nosniff')
      response.setHeader('Cache-Control','private, max-age=31536000, immutable')
      if(range===false){response.setHeader('Content-Range',`bytes */${info.size}`);return response.status(416).end()}
      if(range){
        response.status(206);response.setHeader('Content-Range',`bytes ${range.start}-${range.end}/${info.size}`);response.setHeader('Content-Length',String(range.end-range.start+1))
        if(headOnly)return response.end()
        return createReadStream(path,{start:range.start,end:range.end}).pipe(response)
      }
      response.setHeader('Content-Length',String(info.size))
      if(headOnly)return response.end()
      return createReadStream(path).pipe(response)
    }catch{return response.status(404).end()}
  }
}
