import { forwardRef, KeyboardEvent, useEffect, useImperativeHandle, useRef, useState } from 'react'

// Yazı kutusu artık "**kalın**" gibi işaretler yerine sonucun kendisini gösterir.
// Dışarıya hâlâ markdown veriyoruz; mesaj biçimi ve MessageRichText değişmedi.
export function markdownToHtml(value:string){
  const escaped=value.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;')
  return escaped
    .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,'<a href="$2">$1</a>')
    .replace(/\*\*([^*\n]+)\*\*/g,'<b>$1</b>')
    .replace(/\*([^*\n]+)\*/g,'<i>$1</i>')
    .replace(/\n/g,'<br>')
}
function serialize(node:ChildNode):string{
  if(node.nodeType===Node.TEXT_NODE)return node.textContent||''
  if(node.nodeType!==Node.ELEMENT_NODE)return ''
  const element=node as HTMLElement
  const inner=Array.from(element.childNodes).map(serialize).join('')
  switch(element.tagName){
    case 'BR': return '\n'
    case 'B': case 'STRONG': return inner.trim()?`**${inner}**`:inner
    case 'I': case 'EM': return inner.trim()?`*${inner}*`:inner
    case 'A': { const href=element.getAttribute('href')||'';return href&&inner?`[${inner}](${href})`:inner }
    case 'DIV': case 'P': return inner+'\n'
    default: return inner
  }
}
export function htmlToMarkdown(root:HTMLElement){return Array.from(root.childNodes).map(serialize).join('').replace(/\n+$/,'')}

function caretToEnd(node:HTMLElement){
  const range=document.createRange();range.selectNodeContents(node);range.collapse(false)
  const selection=window.getSelection();selection?.removeAllRanges();selection?.addRange(range)
}

export type RichFormatState={bold:boolean;italic:boolean;link:boolean}
export type RichHandle={focus:()=>void;insert:(text:string)=>void;format:(command:'bold'|'italic')=>void;openLink:()=>void}

const RichInput=forwardRef<RichHandle,{value:string;onChange:(markdown:string)=>void;placeholder:string;disabled?:boolean;onKeyDown?:(e:KeyboardEvent<HTMLDivElement>)=>void;onEnter?:()=>void;onStateChange?:(state:RichFormatState)=>void;onFiles?:(files:File[])=>void}>(
function RichInput({value,onChange,placeholder,disabled,onKeyDown,onEnter,onStateChange,onFiles},ref){
  const editor=useRef<HTMLDivElement>(null),emitted=useRef(''),savedRange=useRef<Range|null>(null),pendingCaret=useRef(false)
  const[linkOpen,setLinkOpen]=useState(false),[url,setUrl]=useState(''),[dragging,setDragging]=useState(false)
  const notify=useRef(onStateChange);notify.current=onStateChange

  // Seçim nerede olursa olsun butonların "aktif mi" durumu güncel kalsın.
  function reportState(){
    const node=editor.current;if(!node||!notify.current)return
    const selection=window.getSelection()
    if(!selection||!selection.anchorNode||!node.contains(selection.anchorNode))return
    let element:Node|null=selection.anchorNode,link=false
    while(element&&element!==node){if((element as HTMLElement).tagName==='A'){link=true;break}element=element.parentNode}
    notify.current({bold:document.queryCommandState('bold'),italic:document.queryCommandState('italic'),link})
  }
  useEffect(()=>{document.addEventListener('selectionchange',reportState);return()=>document.removeEventListener('selectionchange',reportState)},[])

  useEffect(()=>{
    const node=editor.current
    if(!node||value===emitted.current)return
    node.innerHTML=markdownToHtml(value)
    emitted.current=value
    // İçerik dışarıdan değiştiyse (etiket seçimi, alıntı, düzenleme) imleç sona gider;
    // kutu o an odakta değilse odağı aldığında sona alınmak üzere işaretlenir.
    if(document.activeElement===node)caretToEnd(node)
    else pendingCaret.current=true
  },[value])

  function emit(){const node=editor.current;if(!node)return;const markdown=htmlToMarkdown(node);emitted.current=markdown;onChange(markdown);reportState()}
  function focusEditor(){const node=editor.current;if(!node)return;node.focus();if(pendingCaret.current||!node.contains(window.getSelection()?.anchorNode||null)){caretToEnd(node);pendingCaret.current=false}}
  function run(command:string,argument?:string){focusEditor();document.execCommand('styleWithCSS',false,'false');document.execCommand(command,false,argument);emit();reportState()}
  function applyLink(){
    const target=url.trim()
    setLinkOpen(false);setUrl('')
    if(!target)return
    const href=/^https?:\/\//i.test(target)?target:`https://${target}`
    focusEditor()
    const selection=window.getSelection()
    if(savedRange.current){selection?.removeAllRanges();selection?.addRange(savedRange.current)}
    if(selection?.isCollapsed)document.execCommand('insertHTML',false,`<a href="${href.replace(/"/g,'&quot;')}">${href.replace(/</g,'&lt;')}</a>&nbsp;`)
    else document.execCommand('createLink',false,href)
    emit()
  }

  useImperativeHandle(ref,()=>({
    focus:focusEditor,
    insert:(text:string)=>{focusEditor();document.execCommand('insertText',false,text);emit()},
    format:(command:'bold'|'italic')=>run(command),
    openLink:()=>{const selection=window.getSelection();savedRange.current=selection&&selection.rangeCount?selection.getRangeAt(0).cloneRange():null;setLinkOpen(true)},
  }))

  return <div className="composer-editor-wrap">
    {linkOpen&&<div className="composer-link-bar">
      <input autoFocus value={url} onChange={e=>setUrl(e.target.value)} placeholder="https://ornek.com" onKeyDown={e=>{if(e.key==='Enter'){e.preventDefault();applyLink()}else if(e.key==='Escape'){e.preventDefault();e.stopPropagation();setLinkOpen(false);setUrl('');focusEditor()}}}/>
      <button type="button" onMouseDown={e=>e.preventDefault()} onClick={applyLink}>Ekle</button>
      <button type="button" className="ghost" onMouseDown={e=>e.preventDefault()} onClick={()=>{setLinkOpen(false);setUrl('')}}>Vazgeç</button>
    </div>}
    <div ref={editor} className={`composer-editor${dragging?' file-dragging':''}`} role="textbox" aria-multiline="true" aria-label={placeholder} data-placeholder={dragging?'Dosyaları buraya bırak':placeholder}
      contentEditable={!disabled} suppressContentEditableWarning
      onInput={emit}
      onDragEnter={e=>{if(e.dataTransfer.types.includes('Files')){e.preventDefault();setDragging(true)}}}
      onDragOver={e=>{if(e.dataTransfer.types.includes('Files')){e.preventDefault();e.dataTransfer.dropEffect='copy';setDragging(true)}}}
      onDragLeave={e=>{if(!e.currentTarget.contains(e.relatedTarget as Node|null))setDragging(false)}}
      onDrop={e=>{const files=Array.from(e.dataTransfer.files);if(files.length){e.preventDefault();setDragging(false);onFiles?.(files)}}}
      onPaste={e=>{
        e.preventDefault()
        const files=Array.from(e.clipboardData.files)
        if(files.length){onFiles?.(files);return}
        const text=e.clipboardData.getData('text/plain')
        const looksLikeCode=text.includes('\n')&&(/[{};]\s*$/.test(text.trim())||/^\s*(const|let|var|function|class|import|export|def|public|private|SELECT|INSERT|<\/?[a-z])/m.test(text))
        document.execCommand('insertText',false,looksLikeCode?`\`\`\`\n${text.trim()}\n\`\`\``:text)
        emit()
      }}
      onKeyDown={e=>{
        onKeyDown?.(e)
        if(e.defaultPrevented)return
        if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();onEnter?.()}
        else if(e.key==='Enter'&&e.shiftKey){e.preventDefault();document.execCommand('insertLineBreak');emit()}
        else if((e.ctrlKey||e.metaKey)&&(e.key==='b'||e.key==='i')){e.preventDefault();run(e.key==='b'?'bold':'italic')}
      }}/>
  </div>
})
export default RichInput
