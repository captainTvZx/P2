import { BadRequestException, Controller, Delete, ForbiddenException, Get, Inject, NotFoundException, Param, Patch, Post, Put, Req, UseGuards } from '@nestjs/common'
import { IsIn, IsObject, IsOptional, IsString, Length } from 'class-validator'
import { AuthGuard } from './auth'
import { PrismaService } from './prisma.service'
import { canViewProject, requireProjectAccess } from './permissions'
import { RealtimeGateway } from './realtime'
import { Dto } from './validation'

const DOCUMENT_KINDS=['CANVAS','PAGE'] as const
const DOCUMENT_ACCESS=['READ','WRITE'] as const

export class CreateDocumentDto {
  @IsString() @Length(1,100) title!:string
  @IsString() @IsIn(DOCUMENT_KINDS as unknown as string[]) kind!:string
}

export class UpdateDocumentDto {
  @IsOptional() @IsString() @Length(1,100) title?:string
  @IsOptional() @IsObject() content?:Record<string,unknown>
}

export class DocumentAccessDto {
  @IsString() @IsIn(DOCUMENT_ACCESS as unknown as string[]) access!:string
}

const documentInclude={
  owner:{select:{id:true,displayName:true,username:true,avatarUrl:true}},
  permissions:{select:{userId:true,access:true}},
} as const

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

  @Post()
  async create(@Param('organizationId') organizationId:string,@Param('projectId') projectId:string,@Dto(CreateDocumentDto) dto:CreateDocumentDto,@Req() req:any){
    const userId=req.user.sub
    await this.project(organizationId,projectId,userId)
    if(!dto.title.trim())throw new BadRequestException({code:'DOCUMENT_TITLE',message:'Doküman adı boş bırakılamaz.'})
    const content=dto.kind==='PAGE'?{version:1,body:''}:{version:1,nodes:[],viewport:{x:0,y:0,zoom:1}}
    const document=await this.db.projectDocument.create({data:{projectId,ownerId:userId,title:dto.title.trim(),kind:dto.kind,content},include:documentInclude})
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
    const data:any={...(dto.title?{title:dto.title.trim()}:{}),...(dto.content?{content:dto.content}: {})}
    const updated=await this.db.projectDocument.update({where:{id:document.id},data,include:documentInclude})
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
