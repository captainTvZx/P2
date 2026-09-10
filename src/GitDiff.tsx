import { useMemo } from 'react'

// Birleşik farkı satır satır boyar. Eklenen/silinen satırlar ile parça başlıkları
// ayrı okunur; dosya başlıkları (diff --git, index …) gürültü olduğu için atılır.
type Line={kind:'add'|'del'|'ctx'|'hunk'|'meta';text:string;old:number|null;now:number|null}

function parse(diff:string):Line[]{
  const lines:Line[]=[]
  let oldNo=0,newNo=0
  for(const raw of diff.split('\n')){
    if(raw.startsWith('diff --git')||raw.startsWith('index ')||raw.startsWith('new file')||raw.startsWith('deleted file')||raw.startsWith('similarity ')||raw.startsWith('rename ')||raw.startsWith('--- ')||raw.startsWith('+++ ')){
      if(raw.startsWith('diff --git'))lines.push({kind:'meta',text:raw.replace('diff --git ',''),old:null,now:null})
      continue
    }
    if(raw.startsWith('@@')){
      const match=/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/.exec(raw)
      if(match){oldNo=Number(match[1]);newNo=Number(match[2])}
      lines.push({kind:'hunk',text:match?.[3]?.trim()||raw,old:null,now:null})
      continue
    }
    if(raw.startsWith('+')){lines.push({kind:'add',text:raw.slice(1),old:null,now:newNo++});continue}
    if(raw.startsWith('-')){lines.push({kind:'del',text:raw.slice(1),old:oldNo++,now:null});continue}
    if(raw.startsWith('\\'))continue
    lines.push({kind:'ctx',text:raw.slice(1),old:oldNo++,now:newNo++})
  }
  while(lines.length&&!lines[lines.length-1].text&&lines[lines.length-1].kind==='ctx')lines.pop()
  return lines
}

export default function GitDiff({diff,empty}:{diff:string;empty:string}){
  const lines=useMemo(()=>parse(diff||''),[diff])
  const stats=useMemo(()=>({
    add:lines.filter(line=>line.kind==='add').length,
    del:lines.filter(line=>line.kind==='del').length,
  }),[lines])
  if(!diff.trim())return <p className="git-empty">{empty}</p>
  return <div className="git-diff">
    <header><b className="add">+{stats.add}</b><b className="del">−{stats.del}</b></header>
    <div className="git-diff-body">
      {lines.map((line,index)=><div key={index} className={`git-line ${line.kind}`}>
        <i>{line.old ?? ''}</i><i>{line.now ?? ''}</i>
        <span>{line.kind==='add'?'+':line.kind==='del'?'−':line.kind==='hunk'?'@':' '}</span>
        <code>{line.text||' '}</code>
      </div>)}
    </div>
  </div>
}
