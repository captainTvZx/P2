import { BadRequestException, Controller, Delete, ForbiddenException, Get, Inject, NotFoundException, Param, Patch, Post, Put, Req, UseGuards } from '@nestjs/common'
import { IsIn, IsObject, IsOptional, IsString, Length } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { canViewProject, requireProjectAccess } from './permissions'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'
import { mentionedUserIds, recordActivity } from './activity'

const DOCUMENT_KINDS=['CANVAS','PAGE'] as const
const DOCUMENT_ACCESS=['READ','WRITE'] as const

export class CreateDocumentDto {
  @IsString() @Length(1,100) title!:string
  @IsString() @IsIn(DOCUMENT_KINDS as unknown as string[]) kind!:string
  @IsOptional() @IsString() folderId?:string|null
}

export class UpdateDocumentDto {
  @IsOptional() @IsString() @Length(1,100) title?:string
  @IsOptional() @IsObject() content?:Record<string,unknown>
  @IsOptional() @IsString() folderId?:string|null
}

export class DocumentFolderDto {
  @IsString() @Length(1,80) name!:string
}

export class DocumentAccessDto {
  @IsString() @IsIn(DOCUMENT_ACCESS as unknown as string[]) access!:string
}

const documentInclude={
  owner:{select:{id:true,displayName:true,username:true,avatarUrl:true}},
  permissions:{select:{userId:true,access:true}},
} as const

function documentText(value:unknown):string{
  if(typeof value==='string')return value.replace(/<[^>]+>/g,' ')
  if(Array.isArray(value))return value.map(documentText).join(' ')
  if(value&&typeof value==='object')return Object.values(value as Record<string,unknown>).map(documentText).join(' ')
  return ''
}

@UseGuards(AuthGuard)
@Controller('organizations/:organizationId/projects/:projectId/documents')
export class DocumentsController {
  constructor(@Inject(PrismaService) private db:PrismaService,@Inject(RealtimeGateway) private stream:RealtimeGateway){}

  private async project(organizationId:string,projectId:string,userId:string){
    await requireProjectAccess(this.db,organizationId,userId,projectId)
    const project=await this.db.project.findFirst({where:{id:projectId,organizationId,deletedAt:null},select:{id:true}})
    if(!project)throw new NotFoundException({code:'PROJECT_NOT_FOUND',message:'Proje bulunamadı.'})
  }

  private async document(organizationId:string,projectId:string,documentId:string,userId:string){
    await this.project(organizationId,projectId,userId)
    const document=await this.db.projectDocument.findFirst({where:{id:documentId,projectId},include:documentInclude})
    if(!document)throw new NotFoundException({code:'DOCUMENT_NOT_FOUND',message:'Doküman bulunamadı.'})
    const access=document.ownerId===userId?'OWNER':document.permissions.find(row=>row.userId===userId)?.access
    if(!access)throw new NotFoundException({code:'DOCUMENT_NOT_FOUND',message:'Doküman bulunamadı.'})
    return {document,access}
  }

  private view(document:any,userId:string){
    const access=document.ownerId===userId?'OWNER':document.permissions.find((row:any)=>row.userId===userId)?.access
    return {...document,access,permissions:undefined}
  }

  private async audience(documentId:string){
    const row=await this.db.projectDocument.findUnique({where:{id:documentId},select:{ownerId:true,permissions:{select:{userId:true}}}})
    return row?[row.ownerId,...row.permissions.map(permission=>permission.userId)]:[]
  }

  private async folder(projectId:string,folderId:string){
    const folder=await this.db.projectDocumentFolder.findFirst({where:{id:folderId,projectId}})
    if(!folder)throw new BadRequestException({code:'DOCUMENT_FOLDER_NOT_FOUND',message:'Doküman klasörü bulunamadı.'})
    return folder
  }

  @Get()
  async list(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Req() req:any){
    const userId=req.user.sub
    await this.project(organizationId,projectId,userId)
    const documents=await this.db.projectDocument.findMany({
      where:{projectId,OR:[{ownerId:userId},{permissions:{some:{userId}}}]},
      include:documentInclude,orderBy:{updatedAt:'desc'},
    })
    return documents.map(document=>this.view(document,userId))
  }

  @Get('folders')
  async folders(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Req() req:any){
    await this.project(organizationId,projectId,req.user.sub)
    return this.db.projectDocumentFolder.findMany({where:{projectId},orderBy:[{position:'asc'},{createdAt:'asc'}]})
  }

  @Post('folders')
  async createFolder(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Dto(DocumentFolderDto) dto:DocumentFolderDto,@Req() req:any){
    await this.project(organizationId,projectId,req.user.sub)
    const name=dto.name.trim()
    if(!name)throw new BadRequestException({code:'DOCUMENT_FOLDER_NAME',message:'Klasör adı boş bırakılamaz.'})
    const duplicate=await this.db.projectDocumentFolder.findFirst({where:{projectId,name:{equals:name,mode:'insensitive'}}})
    if(duplicate)throw new BadRequestException({code:'DOCUMENT_FOLDER_EXISTS',message:'Bu adda bir klasör zaten var.'})
    const position=await this.db.projectDocumentFolder.count({where:{projectId}})
    return this.db.projectDocumentFolder.create({data:{projectId,name,position}})
  }

  @Patch('folders/:folderId')
  async renameFolder(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('folderId') folderId:string,@Dto(DocumentFolderDto) dto:DocumentFolderDto,@Req() req:any){
    await this.project(organizationId,projectId,req.user.sub)
    await this.folder(projectId,folderId)
    const name=dto.name.trim()
    if(!name)throw new BadRequestException({code:'DOCUMENT_FOLDER_NAME',message:'Klasör adı boş bırakılamaz.'})
    const duplicate=await this.db.projectDocumentFolder.findFirst({where:{projectId,id:{not:folderId},name:{equals:name,mode:'insensitive'}}})
    if(duplicate)throw new BadRequestException({code:'DOCUMENT_FOLDER_EXISTS',message:'Bu adda bir klasör zaten var.'})
    return this.db.projectDocumentFolder.update({where:{id:folderId},data:{name}})
  }

  @Delete('folders/:folderId')
  async deleteFolder(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('folderId') folderId:string,@Req() req:any){
    await this.project(organizationId,projectId,req.user.sub)
    await this.folder(projectId,folderId)
    await this.db.$transaction([this.db.projectDocument.updateMany({where:{projectId,folderId},data:{folderId:null}}),this.db.projectDocumentFolder.delete({where:{id:folderId}})])
    return {ok:true}
  }

  @Post()
  async create(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Dto(CreateDocumentDto) dto:CreateDocumentDto,@Req() req:any){
    const userId=req.user.sub
    await this.project(organizationId,projectId,userId)
    if(!dto.title.trim())throw new BadRequestException({code:'DOCUMENT_TITLE',message:'Doküman adı boş bırakılamaz.'})
    const content=dto.kind==='PAGE'?{version:1,body:''}:{version:1,nodes:[],viewport:{x:0,y:0,zoom:1}}
    if(dto.folderId)await this.folder(projectId,dto.folderId)
    const document=await this.db.projectDocument.create({data:{projectId,ownerId:userId,title:dto.title.trim(),kind:dto.kind,content,folderId:dto.folderId||null},include:documentInclude})
    this.stream.emitToUsers([userId],'document:created',{projectId,document:this.view(document,userId)})
    return this.view(document,userId)
  }

  @Get(':documentId')
  async get(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('documentId') documentId:string,@Req() req:any){
    const {document}=await this.document(organizationId,projectId,documentId,req.user.sub)
    return this.view(document,req.user.sub)
  }

  @Patch(':documentId')
  async update(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('documentId') documentId:string,@Dto(UpdateDocumentDto) dto:UpdateDocumentDto,@Req() req:any){
    const userId=req.user.sub,{document,access}=await this.document(organizationId,projectId,documentId,userId)
    if(access==='READ')throw new ForbiddenException({code:'DOCUMENT_READ_ONLY',message:'Bu doküman yalnızca okunabilir.'})
    if(dto.title!==undefined&&!dto.title.trim())throw new BadRequestException({code:'DOCUMENT_TITLE',message:'Doküman adı boş bırakılamaz.'})
    if(dto.content&&JSON.stringify(dto.content).length>5_000_000)throw new BadRequestException({code:'DOCUMENT_TOO_LARGE',message:'Doküman içeriği çok büyük.'})
    if(dto.folderId)await this.folder(projectId,dto.folderId)
    const data:any={...(dto.title?{title:dto.title.trim()}:{}),...(dto.content?{content:dto.content}: {}),...(dto.folderId!==undefined?{folderId:dto.folderId||null}:{})}
    const updated=await this.db.projectDocument.update({where:{id:document.id},data,include:documentInclude})
    if(dto.content){
      const [before,after]=await Promise.all([mentionedUserIds(this.db,organizationId,documentText(document.content)),mentionedUserIds(this.db,organizationId,documentText(dto.content))])
      const audience=new Set([document.ownerId,...document.permissions.map(row=>row.userId)])
      const targets=after.filter(userId=>!before.includes(userId)&&audience.has(userId))
      await recordActivity(this.db,this.stream,targets.map(userId=>({organizationId,userId,actorId:req.user.sub,kind:'MENTION' as const,preview:documentText(dto.content!),projectId,documentId,channelName:document.title})))
    }
    this.stream.emitToUsers(await this.audience(documentId),'document:updated',{projectId,documentId,editorId:userId})
    return this.view(updated,userId)
  }

  @Delete(':documentId')
  async remove(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('documentId') documentId:string,@Req() req:any){
    const userId=req.user.sub,{document,access}=await this.document(organizationId,projectId,documentId,userId)
    if(access!=='OWNER')throw new ForbiddenException({code:'DOCUMENT_OWNER_ONLY',message:'Dokümanı yalnızca sahibi silebilir.'})
    const audience=await this.audience(documentId)
    await this.db.projectDocument.delete({where:{id:document.id}})
    this.stream.emitToUsers(audience,'document:deleted',{projectId,documentId})
    return {ok:true}
  }

  @Get(':documentId/permissions')
  async permissions(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('documentId') documentId:string,@Req() req:any){
    const {document,access}=await this.document(organizationId,projectId,documentId,req.user.sub)
    if(access!=='OWNER')throw new ForbiddenException({code:'DOCUMENT_OWNER_ONLY',message:'Paylaşımı yalnızca doküman sahibi yönetebilir.'})
    const rows=await this.db.organizationMember.findMany({
      where:{organizationId,OR:[{role:'OWNER'},{user:{memberships:{some:{projectId}}}},{user:{ownedProjects:{some:{id:projectId,organizationId}}}}]},
      orderBy:{joinedAt:'asc'},select:{user:{select:{id:true,displayName:true,username:true,avatarUrl:true}}},
    })
    return {owner:document.owner,permissions:document.permissions,members:rows.map(row=>row.user)}
  }

  @Put(':documentId/permissions/:userId')
  async grant(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('documentId') documentId:string,@Param('userId') targetId:string,@Dto(DocumentAccessDto) dto:DocumentAccessDto,@Req() req:any){
    const {document,access}=await this.document(organizationId,projectId,documentId,req.user.sub)
    if(access!=='OWNER')throw new ForbiddenException({code:'DOCUMENT_OWNER_ONLY',message:'Paylaşımı yalnızca doküman sahibi yönetebilir.'})
    if(targetId===document.ownerId)throw new BadRequestException({code:'DOCUMENT_OWNER_ACCESS',message:'Doküman sahibi zaten tam yetkili.'})
    if(!await canViewProject(this.db,organizationId,targetId,projectId))throw new BadRequestException({code:'MEMBER_NOT_IN_PROJECT',message:'Bu kişi projeyi göremiyor.'})
    const permission=await this.db.documentPermission.upsert({where:{documentId_userId:{documentId,userId:targetId}},create:{documentId,userId:targetId,access:dto.access},update:{access:dto.access}})
    this.stream.emitToUsers([targetId],'document:access',{projectId,documentId,access:dto.access})
    this.stream.emitToUsers([document.ownerId],'document:permissions',{projectId,documentId})
    return permission
  }

  @Delete(':documentId/permissions/:userId')
  async revoke(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Param('documentId') documentId:string,@Param('userId') targetId:string,@Req() req:any){
    const {document,access}=await this.document(organizationId,projectId,documentId,req.user.sub)
    if(access!=='OWNER')throw new ForbiddenException({code:'DOCUMENT_OWNER_ONLY',message:'Paylaşımı yalnızca doküman sahibi yönetebilir.'})
    await this.db.documentPermission.deleteMany({where:{documentId,userId:targetId}})
    this.stream.emitToUsers([targetId],'document:revoked',{projectId,documentId})
    this.stream.emitToUsers([document.ownerId],'document:permissions',{projectId,documentId})
    return {ok:true}
  }
}
