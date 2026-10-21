// The receiver owns the backing store after transfer. No buffer is retained.
self.onmessage = ({data}) => {
  if (!(data.buffer instanceof ArrayBuffer)) throw Error('Expected transferred ArrayBuffer');
  self.postMessage({id:data.id, bytes:data.buffer.byteLength});
};
self.postMessage({ready:true});
