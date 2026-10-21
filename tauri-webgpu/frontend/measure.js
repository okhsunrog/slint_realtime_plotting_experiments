// Opt-in frame timing probe. It runs identically in WebKitGTK and CEF.
(() => {
  const query = new URLSearchParams(location.search);
  if (!query.has('measure')) return;
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const raf = window.requestAnimationFrame.bind(window);
  const probe = {active:false, frames:[], motion:[], longtasks:[], errors:[], longtaskSupported:false};
  window.__wavebenchMotion = (timestamp,head,received,offset) => { if (probe.active) probe.motion.push([timestamp,head,received,offset]); };
  window.requestAnimationFrame = callback => raf(timestamp => {
    const start = performance.now();
    try { return callback(timestamp); }
    finally { if (probe.active) probe.frames.push([timestamp, start, performance.now()]); }
  });
  addEventListener('error', event => probe.errors.push(String(event.message)));
  addEventListener('unhandledrejection', event => probe.errors.push(String(event.reason)));
  if (globalThis.PerformanceObserver?.supportedEntryTypes?.includes('longtask')) {
    new PerformanceObserver(list => {
      if (probe.active) for (const entry of list.getEntries()) probe.longtasks.push([entry.startTime, entry.duration]);
    }).observe({entryTypes:['longtask']});
    probe.longtaskSupported = true;
  }
  const percentile = (values, q) => {
    const sorted = [...values].sort((a,b) => a-b);
    return sorted.length ? sorted[Math.round((sorted.length-1)*q)] : null;
  };
  const button = text => [...document.querySelectorAll('button')].find(b => b.textContent === text);
  async function run() {
    let ready = false;
    for (let i=0; i<120; ++i) {
      if (button('Apply / restart') && !button('Apply / restart').disabled) {ready=true;break;}
      await sleep(250);
    }
    if (!ready) throw Error('Initialization did not finish: '+document.body.innerText);
    const canvas = document.querySelector('#plot');
    if (!(devicePixelRatio > 0)) throw Error('Invalid DPR: '+devicePixelRatio);
    // Match physical rendering work across engines, including their scrollbar differences.
    canvas.style.flexShrink = '0';
    canvas.style.width = `${1280/devicePixelRatio+2}px`;
    canvas.style.height = `${368/devicePixelRatio+2}px`;
    canvas.parentElement.style.height = canvas.style.height;
    document.querySelector('#transport').value = 'ws';
    document.querySelector('#rate').value = query.get('rate') || '1000000';
    document.querySelector('#batch').value = query.get('batch') || '16384';
    button('Apply / restart').click();
    await sleep(5000);
    let graphics = null;
    if (query.get('renderer') === 'webgl') {
      const gl = canvas.getContext('webgl2');
      const ext = gl?.getExtension('WEBGL_debug_renderer_info');
      graphics = {renderer:gl?.getParameter(ext ? ext.UNMASKED_RENDERER_WEBGL : gl.RENDERER), vendor:gl?.getParameter(ext ? ext.UNMASKED_VENDOR_WEBGL : gl.VENDOR), version:gl?.getParameter(gl.VERSION)};
    }
    const startText = document.querySelector('.telemetry').innerText;
    probe.active = true;
    const start = performance.now();
    await sleep(Number(query.get('seconds') || 30)*1000);
    probe.active = false;
    const intervals = probe.frames.slice(1).map((f,i) => f[0]-probe.frames[i][0]);
    const cpu = probe.frames.map(f => f[2]-f[1]);
    const report = {
      tag:query.get('measure'), query:location.search, start, end:performance.now(),
      motion:probe.motion, frames:probe.frames, longtasks:probe.longtasks, errors:probe.errors,
      longtaskSupported:probe.longtaskSupported, graphics,
      viewport:[innerWidth,innerHeight],canvas:[canvas.width,canvas.height],dpr:devicePixelRatio,visibility:document.visibilityState,
      startText,endText:document.querySelector('.telemetry').innerText,
      status:document.querySelector('footer').innerText,
      summary:{frames:probe.frames.length,raf_p50:percentile(intervals,.5),raf_p95:percentile(intervals,.95),raf_p99:percentile(intervals,.99),raf_max:intervals.length ? Math.max(...intervals) : null,
        over25:intervals.filter(x=>x>25).length,over50:intervals.filter(x=>x>50).length,over100:intervals.filter(x=>x>100).length,
        callback_p95:percentile(cpu,.95),callback_p99:percentile(cpu,.99),callback_max:cpu.length ? Math.max(...cpu) : null},
      method:'5 s warmup, wall-clock sample; RAF intervals and callback CPU time, not compositor presentation timing'
    };
    await window.__TAURI__.core.invoke('save_benchmark',{report});
    console.info('FRAME_MEASUREMENT_COMPLETE',JSON.stringify(report.summary));
  }
  run().catch(async error => {
    console.error(error);
    try {await window.__TAURI__.core.invoke('save_benchmark',{report:{tag:query.get('measure'),fatal:String(error),errors:probe.errors,status:document.body.innerText}});} catch {}
  });
})();
