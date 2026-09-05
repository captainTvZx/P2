import { BadRequestException, Controller, Get, Param, Post, Req, Res, UseGuards } from '@nestjs/common'
import { randomUUID } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { extname, join } from 'node:path'
import { AuthGuard } from './auth'

const root=process.env.P2_UPLOAD_DIR||join(process.cwd(),'.p2-data','uploads')
const safeName=(value:string)=>value.replace(/[\x00-\x1f\\/:*?"<>|]/g,'_').slice(0,180)||'dosya'
const allowedExt=(name:string)=>extname(name).replace(/[^a-zA-Z0-9.]/g,'').slice(0,12)

@Controller('uploads')
export class UploadsController {
  @UseGuards(AuthGuard)
  @Post()
  async upload(@Req() req:any){
    const name=safeName(String(req.body?.name||'dosya'))
    const type=String(req.body?.type||'application/octet-stream').slice(0,120)
    const encoded=String(req.body?.data||'')
    const match=/^data:[^;]*;base64,(.+)$/.exec(encoded)
    if(!match)throw new BadRequestException({code:'INVALID_FILE',message:'Dosya verisi geçersiz.'})
    const buffer=Buffer.from(match[1],'base64')
    if(!buffer.length||buffer.length>250*1024*1024)throw new BadRequestException({code:'FILE_SIZE',message:'Dosya 250 MB sınırını aşıyor.'})
    await mkdir(root,{recursive:true})
    const id=randomUUID()+allowedExt(name)
    await writeFile(join(root,id),buffer,{flag:'wx'})
    const meta={name,type,size:buffer.length,userId:req.user.sub}
    await writeFile(join(root,`${id}.json`),JSON.stringify(meta),'utf8')
    return {name,type,size:buffer.length,url:`/uploads/${id}`}
  }

  @Get(':id')
  async file(@Param('id') id:string,@Res() response:any){
    if(!/^[a-f0-9-]{36}(\.[a-zA-Z0-9]{1,10})?$/.test(id))return response.status(404).end()
    try{
      const [buffer,raw]=await Promise.all([readFile(join(root,id)),readFile(join(root,`${id}.json`),'utf8')])
      const meta=JSON.parse(raw)
      const inline=/^(image\/(png|jpeg|gif|webp|avif)|video\/(mp4|webm|ogg)|application\/pdf)$/.test(meta.type||'')
      response.setHeader('Content-Type',inline?meta.type:'application/octet-stream')
      response.setHeader('Content-Disposition',`${inline?'inline':'attachment'}; filename*=UTF-8''${encodeURIComponent(meta.name||'file')}`)
      response.setHeader('X-Content-Type-Options','nosniff')
      response.setHeader('Cache-Control','private, max-age=31536000, immutable')
      return response.send(buffer)
    }catch{return response.status(404).end()}
  }
}
