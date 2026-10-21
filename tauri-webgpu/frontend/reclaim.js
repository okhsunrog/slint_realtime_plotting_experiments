// Opt-in ownership experiment; WebSocket credits bound outstanding transfers.
(() => {
  const mode = new URLSearchParams(location.search).get('reclaim') || 'main';
  const pending = new Map();
  const perSocket = new WeakMap();
  let worker, next = 0, ready = false, failure = null;
  const stats = {mode, sent:0, completed:0, bytes:0, detached:0, outstanding:0, peak:0, errors:0};
  const abort = error => {
    failure = String(error);
    stats.errors++;
    for (const {socket} of pending.values()) socket.close();
    pending.clear();
    stats.outstanding = 0;
    worker?.terminate();
    dispatchEvent(new ErrorEvent('error', {message:'Buffer reclaim: '+failure}));
  };
  window.__wavebenchReclaim = {
    stats: () => ({...stats, ready, failure}),
    init: async () => {
      if (!['main','worker'].includes(mode)) throw Error('Invalid reclaim mode: '+mode);
      if (mode === 'main') { ready = true; return; }
      await new Promise((resolve, reject) => {
        const timer = setTimeout(() => { abort('Worker startup timeout'); reject(Error(failure)); }, 5000);
        worker = new Worker('reclaim-worker.js');
        worker.onerror = event => { clearTimeout(timer); abort(event.message); reject(Error(failure)); };
        worker.onmessageerror = () => { clearTimeout(timer); abort('Worker message decoding failed'); reject(Error(failure)); };
        worker.onmessage = ({data}) => {
          if (data.ready && !ready) { ready = true; clearTimeout(timer); resolve(); return; }
          const item = pending.get(data.id);
          if (!item || item.bytes !== data.bytes) { abort('Worker receipt mismatch'); return; }
          pending.delete(data.id);
          perSocket.set(item.socket, perSocket.get(item.socket)-1);
          stats.completed++;
          stats.outstanding = pending.size;
          window.__wavebenchFlow?.('receipt', item.seq, item.bytes, stats.outstanding);
          // Old sessions may close while the worker still holds their last packets.
          try {
            if (item.socket.readyState === WebSocket.OPEN) item.socket.send(String(item.seq));
          } catch (error) { abort(error); }
        };
      });
    },
    acknowledge: (buffer, socket, seq) => {
      if (!ready || failure) throw Error(failure || 'Reclaimer not ready');
      if (mode === 'main') { window.__wavebenchFlow?.('consumed', seq, buffer.byteLength, 0); socket.send(String(seq)); return; }
      if (!(buffer instanceof ArrayBuffer) || buffer.byteLength === 0) throw Error('Invalid source buffer');
      const inflight = (perSocket.get(socket) || 0)+1;
      if (inflight > 4) throw Error('Worker exceeded WebSocket credit window');
      const id = next++, bytes = buffer.byteLength;
      pending.set(id, {socket, seq, bytes});
      perSocket.set(socket, inflight);
      window.__wavebenchFlow?.('consumed', seq, bytes, inflight);
      try {
        worker.postMessage({id, buffer}, [buffer]);
        if (buffer.byteLength !== 0) throw Error('Source buffer was not detached');
      } catch (error) { abort(error); throw error; }
      stats.sent++;
      stats.bytes += bytes;
      stats.detached++;
      stats.outstanding = pending.size;
      stats.peak = Math.max(stats.peak, pending.size);
    }
  };
  addEventListener('pagehide', () => worker?.terminate(), {once:true});
})();
