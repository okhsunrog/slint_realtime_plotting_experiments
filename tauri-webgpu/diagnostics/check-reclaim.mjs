import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {Worker as Thread} from 'node:worker_threads';
const frontend=fileURLToPath(new URL('../frontend/', import.meta.url));
const workers=[];
class Worker {
  constructor(file) {
    const source=fs.readFileSync(frontend+file,'utf8');
    this.thread=new Thread(`const {parentPort}=require('node:worker_threads');globalThis.self=globalThis;self.postMessage=data=>parentPort.postMessage(data);parentPort.on('message',data=>self.onmessage({data}));${source}`,{eval:true});
    workers.push(this);
    this.thread.on('message',data=>this.onmessage?.({data}));
    this.thread.on('error',error=>this.onerror?.({message:error.message}));
  }
  postMessage(...args) { this.thread.postMessage(...args); }
  terminate() { return this.thread.terminate(); }
}
const events=[];
function create(mode) {
  const context={window:{},location:{search:'?reclaim='+mode},URLSearchParams,Worker,WebSocket:{OPEN:1},ArrayBuffer,ErrorEvent:class{constructor(type,data){Object.assign(this,data)}},setTimeout,clearTimeout,dispatchEvent:e=>events.push(e),addEventListener:()=>{}};
  vm.runInNewContext(fs.readFileSync(frontend+'reclaim.js','utf8'),context);
  return context.window.__wavebenchReclaim;
}
try {
  const main=create('main');await main.init();
  const immediate={readyState:1,sent:[],send(seq){this.sent.push(seq)}};
  const untouched=new ArrayBuffer(64);main.acknowledge(untouched,immediate,0);
  assert.equal(untouched.byteLength,64);assert.deepEqual(immediate.sent,['0']);assert.equal(workers.length,0);
  const reclaim=create('worker');await reclaim.init();
  const socket={readyState:1,sent:[],send(seq){this.sent.push(seq)}};
  for(let seq=0;seq<4;seq++) {
    const buffer=new ArrayBuffer(196672);const data=new Uint8Array(buffer);data.fill(17+seq);
    const wasmCopy=data.slice();
    reclaim.acknowledge(buffer,socket,seq);
    assert.equal(buffer.byteLength,0);
    assert.equal(wasmCopy[0],17+seq);assert.equal(wasmCopy.at(-1),17+seq);
  }
  assert.equal(socket.sent.length,0,'credit waits for actual worker receipt');
  assert.throws(()=>reclaim.acknowledge(new ArrayBuffer(64),socket,4),/credit window/);
  const deadline=Date.now()+5000;
  while(reclaim.stats().completed<4 && Date.now()<deadline) await new Promise(r=>setTimeout(r,5));
  assert.deepEqual(socket.sent,['0','1','2','3']);
  assert.equal(reclaim.stats().outstanding,0);assert.equal(reclaim.stats().detached,4);
  assert.equal(reclaim.stats().peak,4);assert.equal(reclaim.stats().bytes,4*196672);
  const stale={readyState:3,send(){throw Error('must not ACK a closed connection')}};
  reclaim.acknowledge(new ArrayBuffer(64),stale,5);
  while(reclaim.stats().completed<5 && Date.now()<deadline) await new Promise(r=>setTimeout(r,5));
  assert.equal(reclaim.stats().completed,5);assert.equal(events.length,0);
  console.log('PASS: real Worker transfer, independent copy survives, bounded credits, ordered receipt ACK, closed session, default mode');
} finally { await Promise.all(workers.map(worker=>worker.terminate())); }
