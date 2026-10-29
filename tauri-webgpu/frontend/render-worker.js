import init, * as wasm from './pkg/plot_frontend.js';
let socket, running=false, epoch=0, probe=null, info;
const sendStats=()=>postMessage({type:'stats',value:{...JSON.parse(wasm.worker_snapshot()),...info}});
const fatal=error=>{running=false;socket?.close();postMessage({type:'fatal',error:String(error)});};
function frame(timestamp) {
  if (!running) return;
  try {
    const start=performance.now(), motion=Array.from(wasm.worker_frame(timestamp));
    if (probe) {probe.frames.push([timestamp,start,performance.now()]);probe.motion.push(motion);}
    requestAnimationFrame(frame);
  } catch(error) {fatal(error);}
}
let serial=Promise.resolve();
onmessage=({data})=>{
  serial=serial.then(async()=>{
    const {id,op}=data;let result;
    if(op==='init') {
      await init();
      const adapter=await navigator.gpu.requestAdapter({powerPreference:'high-performance'});
      if(!adapter || adapter.info.isFallbackAdapter!==false)throw Error('Hardware WebGPU required in worker');
      info=JSON.parse(await wasm.worker_init(data.canvas));
      info.gpu=`${adapter.info.vendor} ${adapter.info.architecture} · WebGPU · hardware adapter · Worker`;
      if(typeof requestAnimationFrame!=='function')throw Error('Worker RAF unavailable');
      result={...info,timeOrigin:performance.timeOrigin};
      running=true;requestAnimationFrame(frame);setInterval(sendStats,500);
    } else if(op==='stop') {
      epoch++;if(socket){socket.onmessage=null;socket.onerror=null;socket.onclose=null;socket.close();socket=null;}
    } else if(op==='start') {
      epoch++;const current=epoch;
      if(socket){socket.onmessage=null;socket.onerror=null;socket.onclose=null;socket.close();}
      wasm.worker_start(JSON.stringify(data.config),data.session,data.clockOffset);
      socket=new WebSocket(data.url);socket.binaryType='arraybuffer';
      socket.onmessage=({data:buffer})=>{
        if(current!==epoch)return;
        try {const seq=wasm.worker_receive(buffer);socket.send(String(seq));}
        catch(error){fatal(error);}
      };
      socket.onerror=()=>fatal('Worker WebSocket error');
      socket.onclose=()=>{if(current===epoch && running)fatal('Worker WebSocket closed');};
      sendStats();
    } else if(op==='control') {wasm.worker_control(data.command,...data.args);sendStats();}
    else if(op==='probe-start') {probe={frames:[],motion:[],start:performance.now()};result=JSON.parse(wasm.worker_snapshot());}
    else if(op==='probe-end') {result={...probe,end:performance.now(),snapshot:JSON.parse(wasm.worker_snapshot())};probe=null;}
    else if(op==='snapshot')result={...JSON.parse(wasm.worker_snapshot()),...info};
    else throw Error('Unknown worker operation '+op);
    postMessage({id,result});
  }).catch(error=>{postMessage({id:data.id,error:String(error)});fatal(error);});
};
