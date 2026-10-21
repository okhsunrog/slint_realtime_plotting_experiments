#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2D samples;
uniform ivec4 viewport; // write, visible, offset, columns
uniform vec4 scale; // height, amplitude scale, DPR, texture width
uniform int grid_pass;
out vec4 color;
vec2 peaks(int column, int channel) {
    int lo = column * (viewport.y / viewport.w) + column * (viewport.y % viewport.w) / viewport.w;
    int hi = min(viewport.y, max(lo + 1, (column + 1) * (viewport.y / viewport.w) + (column + 1) * (viewport.y % viewport.w) / viewport.w));
    int start = (viewport.x + 1048576 - viewport.y - viewport.z) % 1048576;
    vec2 mm = vec2(3.4e38, -3.4e38);
    for (int i = lo; i < hi; ++i) {
        int index = ((start + i) % 1048576) * 3 + channel;
        float v = texelFetch(samples, ivec2(index % int(scale.w), index / int(scale.w)), 0).r;
        mm = vec2(min(mm.x, v), max(mm.y, v));
    }
    return mm;
}
void main() {
    if (grid_pass == 1) {
        vec2 xy[3] = vec2[3](vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
        gl_Position = vec4(xy[gl_VertexID], 0., 1.);
        color = vec4(1.);
        return;
    }
    int col = gl_InstanceID / 3;
    int ch = gl_InstanceID % 3;
    vec2 corners[6] = vec2[6](vec2(0.,0.), vec2(1.,0.), vec2(0.,1.), vec2(0.,1.), vec2(1.,0.), vec2(1.,1.));
    vec2 q = corners[gl_VertexID];
    vec2 mm = peaks(col, ch);
    float y = mix(mm.x, mm.y, q.y) * scale.y + (q.y * 2. - 1.) * scale.z / scale.x;
    if (viewport.y <= viewport.w * 4) {
        vec2 next_mm = peaks(min(col + 1, viewport.w - 1), ch);
        y = mix((mm.x + mm.y) * .5, (next_mm.x + next_mm.y) * .5, q.x) * scale.y
          + (q.y * 2. - 1.) * 1.4 * scale.z / scale.x;
    }
    gl_Position = vec4((float(col) + q.x) / float(viewport.w) * 2. - 1., y, 0., 1.);
    vec4 colors[3] = vec4[3](vec4(.12,.84,.92,1.), vec4(1.,.69,.32,1.), vec4(.75,.58,1.,1.));
    color = colors[ch];
}
