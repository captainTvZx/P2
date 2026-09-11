import { useState } from 'react'
import { useEscape } from './useEscape'

// Electron window.prompt() desteklemez; dal/etiket adı ve reset türü gibi
// küçük sorular için uygulamanın kendi görsel dilinde bir pencere.
export type PromptSpec={
  title:string
  body?:string
  input?:{label:string;initial?:string;placeholder?:string}
  choices:{label:string;value:string;danger?:boolean;primary?:boolean}[]
}
export type PromptAnswer={choice:string;value:string}

export default function GitPrompt({spec,onAnswer,onCancel}:{spec:PromptSpec;onAnswer:(answer:PromptAnswer)=>void;onCancel:()=>void}){
  const[value,setValue]=useState(spec.input?.initial||'')
  useEscape(true,onCancel)
  const primary=spec.choices.find(choice=>choice.primary)||spec.choices[spec.choices.length-1]
  const canSubmit=!spec.input||value.trim().length>0
  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onCancel()}}>
    <form className="share-modal confirm-modal git-prompt" onSubmit={event=>{event.preventDefault();if(canSubmit)onAnswer({choice:primary.value,value:value.trim()})}}>
      <header><div><b>{spec.title}</b>{spec.body&&<small>{spec.body}</small>}</div><button type="button" onClick={onCancel} aria-label="Kapat">×</button></header>
      {spec.input&&<label className="git-field"><span>{spec.input.label}</span><input autoFocus value={value} placeholder={spec.input.placeholder} onChange={event=>setValue(event.target.value)}/></label>}
      <footer>
        <button type="button" className="share-cancel" onClick={onCancel}>Vazgeç</button>
        {spec.choices.map(choice=><button type={choice===primary?'submit':'button'} key={choice.value} className={choice.danger?'share-start danger':'share-start'} disabled={!canSubmit} onClick={choice===primary?undefined:()=>onAnswer({choice:choice.value,value:value.trim()})}>{choice.label}</button>)}
      </footer>
    </form>
  </div>
}
