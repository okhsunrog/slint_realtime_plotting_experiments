struct Params { capacity:u32, write:u32, filled:u32, visible:u32, offset:u32, columns:u32, height:u32, channels:u32, y_scale:f32, dpr:f32, pad0:u32, pad1:u32 }
@group(0) @binding(0) var<storage,read> samples:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read_write> peaks:array<vec2f>;
@compute @workgroup_size(64)
fn reduce(@builtin(global_invocation_id) id:vec3u) {
    let col=id.x/p.channels; let ch=id.x%p.channels;
    if col>=p.columns {return;}
    let start=(p.write+p.capacity-p.visible-p.offset)%p.capacity;
    // Quotient/remainder form avoids u32 overflow on wide high-DPI surfaces.
    let lo=col*(p.visible/p.columns)+col*(p.visible%p.columns)/p.columns;
    let end=(col+1u)*(p.visible/p.columns)+(col+1u)*(p.visible%p.columns)/p.columns;
    let hi=min(p.visible,max(lo+1u,end));
    var mn=3.4e38; var mx=-3.4e38;
    for(var i=lo;i<hi;i++) {let v=samples[((start+i)%p.capacity)*p.channels+ch];mn=min(mn,v);mx=max(mx,v);}
    peaks[id.x]=vec2f(mn,mx);
}
