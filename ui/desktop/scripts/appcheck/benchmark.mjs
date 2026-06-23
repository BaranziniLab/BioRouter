import { readFileSync } from 'node:fs';
const base='http://127.0.0.1:3000';
const slug=t=>{let o='',d=false;for(const c of t.trim()){if(/[a-z0-9]/i.test(c)){o+=c.toLowerCase();d=false}else if(!d&&o){o+='-';d=true}}return o.replace(/^-+|-+$/g,'')||'artifact'};
const TYPES=[['select',/br-select|<select/],['slider',/br-slider|type="range"/],['switch',/br-switch/],['check',/br-check|type="checkbox"/],['chip',/br-chip/],['tab',/br-tab\b|br-tabs/],['dropzone',/br-dropzone/],['drag',/br-draglist|br-dragitem/],['region',/br-mapgrid|br-region/],['grid',/br-grid/],['buttons2+',/(<button[\s\S]*){2,}/]];
async function variety(specs){
  let total=0,custom=0,sum=0;
  for(const {spec,title} of specs){
    for(const id of [spec, slug(title)]){
      let html=null; try{const r=await fetch(`${base}/apps/${id}/`); if(r.status===200)html=await r.text();}catch{}
      if(!html)continue;
      total++; const n=TYPES.filter(([_,re])=>re.test(html)).length; sum+=n; if(n>0)custom++; break;
    }
  }
  return {total, custom, avgTypes:(sum/total).toFixed(2)};
}
for(const round of ['round2','round3']){
  const rs=readFileSync(`/Users/wanjun/Desktop/biorouter-apps-wt/scripts/agent-drafter-apps/${round}.sh`,'utf8');
  const specs=[...rs.matchAll(/^"([a-z0-9-]+)\|([^|]+)\|/gm)].map(m=>({spec:m[1],title:m[2]}));
  const r=await variety(specs);
  console.log(`${round} (${round==='round2'?'detailed':'vague'}): served=${r.total} custom-UI=${r.custom}/${r.total} avg distinct control-types/app=${r.avgTypes}`);
}
