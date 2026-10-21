#version 300 es
precision highp float;
precision highp int;
uniform ivec4 viewport;
uniform vec4 scale;
uniform int grid_pass;
in vec4 color;
out vec4 output_color;
void main() {
    if (grid_pass == 0) { output_color = color; return; }
    vec2 xy = gl_FragCoord.xy / vec2(float(viewport.w), scale.x);
    vec2 cells = xy * vec2(10., 8.);
    vec2 distance_to_grid = min(fract(cells), 1. - fract(cells));
    bool line = distance_to_grid.x < 9. / float(viewport.w) || distance_to_grid.y < 7. / scale.x;
    bool zero = abs(xy.y - .5) < .7 * scale.z / scale.x;
    vec3 c = vec3(.035,.075,.115);
    if (line) c = vec3(.085,.15,.205);
    if (zero) c = vec3(.19,.29,.36);
    output_color = vec4(c, 1.);
}
