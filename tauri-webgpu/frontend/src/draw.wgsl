struct Params { capacity:u32, write:u32, filled:u32, visible:u32, offset:u32, columns:u32, height:u32, channels:u32, y_scale:f32, dpr:f32, pad0:u32, pad1:u32 }
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read> peaks:array<vec2f>;
struct Vertex { @builtin(position) position:vec4f, @location(0) color:vec4f }
fn color(ch:u32)->vec4f {let colors=array<vec4f,3>(vec4f(0.12,0.84,0.92,1.),vec4f(1.,0.69,0.32,1.),vec4f(0.75,0.58,1.,1.));return colors[ch];}
@vertex fn wave(@builtin(vertex_index) v:u32,@builtin(instance_index) instance:u32)->Vertex {
    let col=instance/p.channels; let ch=instance%p.channels;
    let mm=peaks[instance]; let next=peaks[min(col+1u,p.columns-1u)*p.channels+ch];
    let corners=array<vec2f,6>(vec2f(0.,0.),vec2f(1.,0.),vec2f(0.,1.),vec2f(0.,1.),vec2f(1.,0.),vec2f(1.,1.));
    let q=corners[v]; var x=(f32(col)+q.x)/f32(p.columns)*2.-1.;
    var y=mix(mm.x,mm.y,q.y)*p.y_scale+(q.y*2.-1.)*p.dpr/f32(p.height);
    // Connect neighbouring extrema in the sparse regime, retain every peak when dense.
    if p.visible<=p.columns*4u {
        y=mix((mm.x+mm.y)*0.5,(next.x+next.y)*0.5,q.x)*p.y_scale+(q.y*2.-1.)*1.4*p.dpr/f32(p.height);
    }
    return Vertex(vec4f(x,y,0.,1.),color(ch));
}
@fragment fn ink(v:Vertex)->@location(0) vec4f {return v.color;}
@vertex fn fullscreen(@builtin(vertex_index) v:u32)->@builtin(position) vec4f {
    let xy=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));return vec4f(xy[v],0.,1.);
}
@fragment fn grid(@builtin(position) pos:vec4f)->@location(0) vec4f {
    let xy=pos.xy/vec2f(f32(p.columns),f32(p.height));
    let cells=xy*vec2f(10.,8.); let distance=min(fract(cells),1.-fract(cells));
    let gridline=distance.x<9./f32(p.columns) || distance.y<7./f32(p.height);
    let zero=abs(xy.y-0.5)<0.7*p.dpr/f32(p.height);
    var c=vec3f(0.035,0.075,0.115);
    if gridline {c=vec3f(0.085,0.15,0.205);} if zero {c=vec3f(0.19,0.29,0.36);}
    return vec4f(c,1.);
}
