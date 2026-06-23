import WebSocket from 'ws';
import { readFileSync } from 'node:fs';
const base = process.argv[2] || 'http://127.0.0.1:3000';
const rs = readFileSync('/Users/wanjun/Desktop/biorouter-apps-wt/scripts/agent-drafter-apps/round.sh','utf8');
const ids = [...rs.matchAll(/^"([a-z0-9-]+)\|/gm)].map(m=>m[1]);
function check(id){return new Promise(async(resolve)=>{
  const r={id,ok:false,err:''};
  try{
    const idx=await fetch(`${base}/apps/${id}/`); const html=await idx.text();
    const b=await fetch(`${base}/apps/${id}/dist/app.js`); const bundle=await b.text();
    if(idx.status!==200||b.status!==200||bundle.length<500||!html.includes('biorouter-theme')){throw new Error(`http idx=${idx.status} bundle=${b.status}`);}
  }catch(e){r.err='http: '+e.message;return resolve(r);}
  const ws=new WebSocket(base.replace(/^http/,'ws')+`/apps/${id}/agent`); let reply='';
  const t=setTimeout(()=>{r.err='timeout';try{ws.close()}catch{};resolve(r);},60000);
  ws.on('message',d=>{let m;try{m=JSON.parse(d)}catch{return}
    if(m.type==='ready')ws.send(JSON.stringify({type:'prompt',text:'In one sentence, what do you do?'}));
    else if(m.type==='message')reply+=m.delta;
    else if(m.type==='error'){clearTimeout(t);r.err=m.message;ws.close();resolve(r);}
    else if(m.type==='done'){clearTimeout(t);r.ok=reply.trim().length>10;if(!r.ok)r.err='empty reply';ws.close();resolve(r);}});
  ws.on('error',e=>{clearTimeout(t);r.err='ws: '+e.message;resolve(r);});
});}
let pass=0;const fails=[];
for(const id of ids){const r=await check(id);if(r.ok)pass++;else fails.push(r);}
console.log(`CHECKLIST: ${pass}/${ids.length} ok`);
for(const f of fails)console.log(`  FAIL ${f.id}: ${f.err}`);
