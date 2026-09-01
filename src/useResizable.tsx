import { MouseEvent as ReactMouseEvent, useEffect, useState } from 'react'

// Sürüklenebilir panel genişliği; seçilen genişlik cihazda hatırlanır.
export function useResizable(key:string,initial:number,min:number,max:number,direction:1|-1){
  const[width,setWidth]=useState(()=>{try{const stored=Number(window.localStorage.getItem(key));return stored>=min&&stored<=max?stored:initial}catch{return initial}})
  useEffect(()=>{try{window.localStorage.setItem(key,String(width))}catch{}},[key,width])
  function start(event:ReactMouseEvent){
    event.preventDefault()
    const startX=event.clientX,startWidth=width
    const move=(e:globalThis.MouseEvent)=>setWidth(Math.max(min,Math.min(max,startWidth+direction*(e.clientX-startX))))
    const stop=()=>{document.removeEventListener('mousemove',move);document.removeEventListener('mouseup',stop);document.body.classList.remove('resizing')}
    document.body.classList.add('resizing')
    document.addEventListener('mousemove',move)
    document.addEventListener('mouseup',stop)
  }
  return{width,start}
}

export function ResizeHandle({edge,onMouseDown,label}:{edge:'left'|'right';onMouseDown:(event:ReactMouseEvent)=>void;label:string}){
  return <div className={`resize-handle ${edge}`} role="separator" aria-orientation="vertical" aria-label={label} title={label} onMouseDown={onMouseDown}/>
}
