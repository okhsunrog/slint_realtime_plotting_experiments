(() => {
  const enabled=new URLSearchParams(location.search).get('execution')==='worker';
  let worker, next=0, latest=null, workerOrigin=0, failure=null;
  const pending=new Map();
  function fatal(error) {
    failure=String(error);
    for(const p of pending.values()){clearTimeout(p.timer);p.reject(Error(failure));}
    pending.clear();worker?.terminate();
    window.__wavebenchWorkerError?.(failure);
    dispatchEvent(new ErrorEvent('error',{message:'Render worker: '+failure}));
  }
  function call(op, data={}, transfer=[]) {
    if(failure)return Promise.reject(Error(failure));
    return new Promise((resolve,reject)=>{
      const id=++next,timer=setTimeout(()=>{pending.delete(id);reject(Error('Worker timeout: '+op));},15000);
      pending.set(id,{resolve,reject,timer});
      try{worker.postMessage({id,op,...data},transfer);}catch(error){clearTimeout(timer);pending.delete(id);reject(error);}
    });
  }
  function resize() {
    const c=document.querySelector('#plot'),dpr=devicePixelRatio;
    return call('control',{command:'resize',args:[Math.round(c.clientWidth*dpr),Math.round(c.clientHeight*dpr),dpr]});
  }
  const api=window.__wavebenchRenderWorker={enabled,
    snapshot:()=>latest,
    init:async()=>{
      if(new URLSearchParams(location.search).get('renderer')==='webgl')throw Error('Render worker requires WebGPU');
      const canvas=document.querySelector('#plot');
      worker=new Worker('render-worker.js',{type:'module'});
      worker.onerror=e=>fatal(e.message);worker.onmessageerror=()=>fatal('Message decoding failed');
      worker.onmessage=({data})=>{
        if(data.type==='fatal'){fatal(data.error);return;}
        if(data.type==='stats'){latest=data.value;window.__wavebenchWorkerUpdate?.(JSON.stringify(latest));return;}
        const p=pending.get(data.id);if(!p)return;
        pending.delete(data.id);clearTimeout(p.timer);data.error?p.reject(Error(data.error)):p.resolve(data.result);
      };
      const offscreen=canvas.transferControlToOffscreen();
      const info=await call('init',{canvas:offscreen},[offscreen]);workerOrigin=info.timeOrigin;
      const observer=new ResizeObserver(()=>resize().catch(fatal));observer.observe(canvas);
      addEventListener('resize',()=>resize().catch(fatal));
      await resize();return JSON.stringify(info);
    },
    start:async configJson=>{
      const config=JSON.parse(configJson);if(config.mode!=='ws')throw Error('Render worker supports Binary WebSocket');
      let best=Infinity,offset=0;
      for(let i=0;i<9;i++){
        const start=performance.now(),backend=await window.__TAURI__.core.invoke('clock'),end=performance.now();
        if(end-start<best){best=end-start;offset=backend-(start+end)/2;}
      }
      await call('stop');
      const channel=new window.__TAURI__.core.Channel(()=>{});
      const response=await window.__TAURI__.core.invoke('start_stream',{config,onBatch:channel});
      await call('start',{...response,session:response.session,config,clockOffset:offset+workerOrigin-performance.timeOrigin});
      await resize();latest=await call('snapshot');window.__wavebenchWorkerUpdate?.(JSON.stringify(latest));
    },
    control:(command,a=0,b=0,c=0)=>call('control',{command,args:[a,b,c]}).catch(fatal),
    probeStart:async()=>{await resize();latest=await call('probe-start');return latest;},
    probeEnd:async()=>{
      const data=await call('probe-end'),delta=workerOrigin-performance.timeOrigin;
      latest={...data.snapshot,...(latest?{gpu:latest.gpu}: {})};
      return {...data,start:data.start+delta,end:data.end+delta,
        frames:data.frames.map(row=>row.map(t=>t+delta)),
        motion:data.motion.map(([t,...rest])=>[t+delta,...rest])};
    },
  };
  addEventListener('pagehide',()=>worker?.terminate(),{once:true});
})();
