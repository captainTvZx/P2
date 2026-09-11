import { useEffect, useMemo, useState } from 'react'
import { ComposerIcon } from './ComposerIcons'

// Merge çakışması çözümleme ekranı. Çalışma kopyasındaki <<<<<<< / ======= / >>>>>>>
// işaretleri bloklara ayrılır; her blok için bizimki / onlarınki / ikisi seçilir
// ya da elle düzenlenir. Sonuç dosyaya yazılıp stage'e alınır; tüm dosyalar
// çözülünce merge commit'i atılır. demo=true iken git'e dokunulmaz.
export type MergeConflict={path:string;base:string;ours:string;theirs:string;current:string}
export type MergeState={merging:boolean;branch:string;mergeHead:string;mergeMessage:string;conflicts:MergeConflict[]}

type Segment={kind:'text';lines:string[]}|{kind:'conflict';ours:string[];theirs:string[];oursLabel:string;theirsLabel:string;choice:'none'|'ours'|'theirs'|'both'|'manual';manual:string}

export function splitConflicts(content:string):Segment[]{
  const segments:Segment[]=[]
  let text:string[]=[]
  const lines=content.replace(/\r\n/g,'\n').split('\n')
  for(let index=0;index<lines.length;index++){
    const line=lines[index]
    if(line.startsWith('<<<<<<<')){
      if(text.length){segments.push({kind:'text',lines:text});text=[]}
      const oursLabel=line.slice(8).trim()||'HEAD'
      const ours:string[]=[],theirs:string[]=[]
      let theirsLabel='',side:'ours'|'theirs'='ours'
      index++
      for(;index<lines.length;index++){
        const inner=lines[index]
        if(inner.startsWith('=======')&&side==='ours'){side='theirs';continue}
        if(inner.startsWith('|||||||')&&side==='ours'){ // diff3 tarzı temel bölümü atlanır
          for(;index<lines.length&&!lines[index].startsWith('=======');index++);
          side='theirs';continue
        }
        if(inner.startsWith('>>>>>>>')){theirsLabel=inner.slice(8).trim()||'incoming';break}
        (side==='ours'?ours:theirs).push(inner)
      }
      segments.push({kind:'conflict',ours,theirs,oursLabel,theirsLabel,choice:'none',manual:''})
      continue
    }
    text.push(line)
  }
  if(text.length)segments.push({kind:'text',lines:text})
  return segments
}

export function joinSegments(segments:Segment[]):string{
  const out:string[]=[]
  for(const segment of segments){
    if(segment.kind==='text'){out.push(...segment.lines);continue}
    if(segment.choice==='ours')out.push(...segment.ours)
    else if(segment.choice==='theirs')out.push(...segment.theirs)
    else if(segment.choice==='both')out.push(...segment.ours,...segment.theirs)
    else if(segment.choice==='manual')out.push(...segment.manual.split('\n'))
    else out.push(`<<<<<<< ${segment.oursLabel}`,...segment.ours,'=======',...segment.theirs,`>>>>>>> ${segment.theirsLabel}`)
  }
  return out.join('\n')
}

export const DEMO_MERGE:MergeState={
  merging:true,branch:'main',mergeHead:'a1b2c3d',mergeMessage:"Merge branch 'feature/tank-formation' into main",
  conflicts:[
    {path:'Source/Tank/TankFormation.cpp',base:'',ours:'',theirs:'',current:[
      '#include "TankFormation.h"','',
      'void ATankFormation::Tick(float DeltaTime)','{','    Super::Tick(DeltaTime);',
      '<<<<<<< HEAD','    UpdateFormation(DeltaTime * FormationSpeed);','    if (bDebugDraw) DrawFormationDebug();','=======','    UpdateFormation(DeltaTime);','    ApplySpacing(FormationSpacing);','>>>>>>> feature/tank-formation',
      '}','',
      'float ATankFormation::GetSpacing() const','{',
      '<<<<<<< HEAD','    return FormationSpacing * SpacingScale;','=======','    return FMath::Max(FormationSpacing, MinSpacing);','>>>>>>> feature/tank-formation',
      '}',''].join('\n')},
    {path:'Config/DefaultGame.ini',base:'',ours:'',theirs:'',current:[
      '[/Script/Engine.GameSession]','MaxPlayers=16','',
      '<<<<<<< HEAD','[Formation]','DefaultSpacing=250','=======','[Formation]','DefaultSpacing=300','MinSpacing=120','>>>>>>> feature/tank-formation',''].join('\n')},
    {path:'Content/Data/Units.csv',base:'',ours:'',theirs:'',current:[
      'Name,Speed,Armor',
      '<<<<<<< HEAD','Tank,42,120','=======','Tank,40,135','Scout,90,30','>>>>>>> feature/tank-formation',''].join('\n')},
  ],
}

export default function MergeScreen({state,demo,busy,onResolve,onFinish,onAbort,onClose}:{
  state:MergeState
  demo?:boolean
  busy?:boolean
  onResolve:(path:string,content:string)=>Promise<boolean>
  onFinish:(message:string)=>void
  onAbort:()=>void
  onClose:()=>void
}){
  const[files,setFiles]=useState(()=>state.conflicts.map(item=>({...item,segments:splitConflicts(item.current),resolved:false})))
  const[selected,setSelected]=useState(0)
  const[message,setMessage]=useState(state.mergeMessage||`Merge into '${state.branch}'`)
  const[editing,setEditing]=useState<number|null>(null)
  useEffect(()=>{setFiles(state.conflicts.map(item=>({...item,segments:splitConflicts(item.current),resolved:false})));setSelected(0)},[state])
  const file=files[selected]
  const remaining=useMemo(()=>files.filter(item=>!item.resolved).length,[files])
  const openBlocks=file?file.segments.filter(segment=>segment.kind==='conflict'&&segment.choice==='none').length:0

  function choose(index:number,choice:'ours'|'theirs'|'both'){
    setFiles(current=>current.map((item,at)=>at!==selected?item:{...item,resolved:false,segments:item.segments.map((segment,position)=>position===index&&segment.kind==='conflict'?{...segment,choice}:segment)}))
  }
  function chooseAll(choice:'ours'|'theirs'|'both'){
    setFiles(current=>current.map((item,at)=>at!==selected?item:{...item,resolved:false,segments:item.segments.map(segment=>segment.kind==='conflict'?{...segment,choice}:segment)}))
  }
  function setManual(index:number,manual:string){
    setFiles(current=>current.map((item,at)=>at!==selected?item:{...item,resolved:false,segments:item.segments.map((segment,position)=>position===index&&segment.kind==='conflict'?{...segment,choice:'manual',manual}:segment)}))
  }
  async function markResolved(){
    if(!file||openBlocks)return
    const content=joinSegments(file.segments)
    const ok=await onResolve(file.path,content)
    if(!ok)return
    setFiles(current=>current.map((item,at)=>at===selected?{...item,resolved:true,current:content}:item))
    const next=files.findIndex((item,at)=>at!==selected&&!item.resolved)
    if(next>=0)setSelected(next)
  }

  return <div className="merge-screen">
    <header className="merge-head">
      <div className="merge-title">
        <ComposerIcon name="branch"/>
        <div><b>{demo?'Merge ekranı — demo':'Çakışmaları çöz'}</b><small>{state.mergeMessage||`'${state.mergeHead}' → '${state.branch}'`}{demo&&' · örnek veriyle, git’e dokunmaz'}</small></div>
      </div>
      <div className="merge-progress"><i style={{width:`${files.length?((files.length-remaining)/files.length)*100:0}%`}}/><span>{files.length-remaining}/{files.length} dosya çözüldü</span></div>
      <button type="button" className="git-button subtle" onClick={onClose}>Kapat</button>
    </header>

    <div className="merge-body">
      <aside className="merge-files">
        <header>Çakışan dosyalar <em>{remaining}</em></header>
        {files.map((item,index)=><button type="button" key={item.path} className={`${index===selected?'active':''}${item.resolved?' resolved':''}`} onClick={()=>setSelected(index)}>
          <i>{item.resolved?'✓':'!'}</i><span title={item.path}>{item.path}</span><small>{item.segments.filter(segment=>segment.kind==='conflict').length} blok</small>
        </button>)}
      </aside>

      <section className="merge-editor">
        {!file?<p className="git-empty">Çakışan dosya kalmadı.</p>:<>
          <header>
            <b>{file.path}</b>
            <span className="merge-bulk">
              <small>Tümü için:</small>
              <button type="button" onClick={()=>chooseAll('ours')}>Bizimki</button>
              <button type="button" onClick={()=>chooseAll('theirs')}>Onlarınki</button>
              <button type="button" onClick={()=>chooseAll('both')}>İkisi de</button>
            </span>
          </header>
          <div className="merge-segments">
            {file.segments.map((segment,index)=>segment.kind==='text'
              ?<pre className="merge-text" key={index}>{segment.lines.join('\n')||' '}</pre>
              :<div className={`merge-conflict choice-${segment.choice}`} key={index}>
                <div className="merge-side ours">
                  <header><b>Bizimki</b><small>{segment.oursLabel}</small><button type="button" className={segment.choice==='ours'?'active':''} onClick={()=>choose(index,'ours')}>Bunu al</button></header>
                  <pre>{segment.ours.join('\n')||'(boş)'}</pre>
                </div>
                <div className="merge-side theirs">
                  <header><b>Onlarınki</b><small>{segment.theirsLabel}</small><button type="button" className={segment.choice==='theirs'?'active':''} onClick={()=>choose(index,'theirs')}>Bunu al</button></header>
                  <pre>{segment.theirs.join('\n')||'(boş)'}</pre>
                </div>
                <footer>
                  <button type="button" className={segment.choice==='both'?'active':''} onClick={()=>choose(index,'both')}>İkisini de al</button>
                  <button type="button" className={segment.choice==='manual'?'active':''} onClick={()=>{setEditing(editing===index?null:index);if(segment.choice!=='manual')setManual(index,[...segment.ours,...segment.theirs].join('\n'))}}>Elle düzenle</button>
                  <em>{segment.choice==='none'?'Seçim bekliyor':segment.choice==='ours'?'Bizimki seçildi':segment.choice==='theirs'?'Onlarınki seçildi':segment.choice==='both'?'İkisi de alındı':'Elle düzenlendi'}</em>
                </footer>
                {(editing===index||segment.choice==='manual')&&<textarea className="merge-manual" value={segment.manual} spellCheck={false} onChange={event=>setManual(index,event.target.value)}/>}
              </div>)}
          </div>
          <footer className="merge-file-actions">
            <small>{openBlocks?`${openBlocks} blok seçim bekliyor`:file.resolved?'Bu dosya çözüldü':'Tüm bloklar seçildi'}</small>
            <button type="button" className="git-button primary" disabled={Boolean(openBlocks)||busy} onClick={()=>void markResolved()}>{file.resolved?'Yeniden kaydet':'Çözüldü olarak işaretle'}</button>
          </footer>
        </>}
      </section>
    </div>

    <footer className="merge-foot">
      <input value={message} onChange={event=>setMessage(event.target.value)} placeholder="Merge commit mesajı"/>
      <button type="button" className="git-button" disabled={busy} onClick={onAbort}>Merge’ü iptal et</button>
      <button type="button" className="git-button primary" disabled={busy||remaining>0} onClick={()=>onFinish(message.trim())}>{remaining>0?`${remaining} dosya bekliyor`:'Merge’ü tamamla'}</button>
    </footer>
  </div>
}
