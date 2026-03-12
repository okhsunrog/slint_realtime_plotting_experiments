struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0,  3.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0)
    );

    let pos = positions[vertex_index];
    var output: VertexOutput;
    output.position = vec4<f32>(pos.x, -pos.y, 0.0, 1.0);
    output.uv = vec2<f32>(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return output;
}

struct PlotParams {
    write_pos: u32,
    num_samples: u32,
    y_min: f32,
    y_max: f32,
    grid_x_divisions: f32,
    grid_y_divisions: f32,
    time_val: f32,
    _padding: f32,
};

var<immediate> params: PlotParams;

@group(0) @binding(0) var<storage, read> samples: array<f32>;

const BG_COLOR: vec3<f32> = vec3<f32>(0.06, 0.06, 0.12);
const GRID_COLOR: vec3<f32> = vec3<f32>(0.15, 0.15, 0.25);
const AXIS_COLOR: vec3<f32> = vec3<f32>(0.4, 0.4, 0.6);
const WAVEFORM_COLOR: vec3<f32> = vec3<f32>(0.1, 0.9, 0.3);
const WAVEFORM_GLOW: vec3<f32> = vec3<f32>(0.05, 0.4, 0.15);

fn get_sample(index: u32) -> f32 {
    let actual_index = (params.write_pos + index) % params.num_samples;
    return samples[actual_index];
}

fn value_to_y(value: f32) -> f32 {
    return (value - params.y_min) / (params.y_max - params.y_min);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    var color = BG_COLOR;

    let margin_left = 0.06;
    let margin_right = 0.02;
    let margin_top = 0.04;
    let margin_bottom = 0.06;

    let plot_uv = vec2<f32>(
        (uv.x - margin_left) / (1.0 - margin_left - margin_right),
        (uv.y - margin_top) / (1.0 - margin_top - margin_bottom)
    );

    if plot_uv.x < 0.0 || plot_uv.x > 1.0 || plot_uv.y < 0.0 || plot_uv.y > 1.0 {
        return vec4<f32>(BG_COLOR * 0.7, 1.0);
    }

    // Grid lines
    let grid_x_spacing = 1.0 / params.grid_x_divisions;
    let grid_x_frac = fract(plot_uv.x / grid_x_spacing);
    let grid_x_dist = min(grid_x_frac, 1.0 - grid_x_frac) * params.grid_x_divisions;

    let grid_y_spacing = 1.0 / params.grid_y_divisions;
    let grid_y_frac = fract(plot_uv.y / grid_y_spacing);
    let grid_y_dist = min(grid_y_frac, 1.0 - grid_y_frac) * params.grid_y_divisions;

    let grid_line_width = 0.015;
    if grid_x_dist < grid_line_width || grid_y_dist < grid_line_width {
        color = GRID_COLOR;
    }

    // Zero line
    let zero_y = value_to_y(0.0);
    let zero_dist = abs(plot_uv.y - (1.0 - zero_y));
    if zero_dist < 0.003 {
        color = AXIS_COLOR;
    }

    // Plot border
    let border_dist = min(
        min(plot_uv.x, 1.0 - plot_uv.x),
        min(plot_uv.y, 1.0 - plot_uv.y)
    );
    if border_dist < 0.002 {
        color = AXIS_COLOR;
    }

    // Waveform
    let sample_x = plot_uv.x * f32(params.num_samples - 1u);
    let sample_index_low = u32(floor(sample_x));
    let sample_index_high = min(sample_index_low + 1u, params.num_samples - 1u);
    let frac = fract(sample_x);

    let val_low = get_sample(sample_index_low);
    let val_high = get_sample(sample_index_high);
    let val = mix(val_low, val_high, frac);

    let waveform_y = 1.0 - value_to_y(val);
    let dist_to_waveform = abs(plot_uv.y - waveform_y);

    // Fill vertical segments for steep transitions
    let y_low = 1.0 - value_to_y(val_low);
    let y_high = 1.0 - value_to_y(val_high);
    let y_min_seg = min(y_low, y_high);
    let y_max_seg = max(y_low, y_high);

    var segment_dist = dist_to_waveform;
    if plot_uv.y >= y_min_seg && plot_uv.y <= y_max_seg {
        let nearest_sample_x = round(sample_x) / f32(params.num_samples - 1u);
        segment_dist = min(segment_dist, abs(plot_uv.x - nearest_sample_x) * f32(params.num_samples));
    }

    let pixel_size = 1.0 / f32(params.num_samples);
    let line_width = 1.5;
    let line_intensity = smoothstep(pixel_size * line_width, 0.0, segment_dist);

    let glow_width = pixel_size * 8.0;
    let glow_intensity = smoothstep(glow_width, 0.0, segment_dist) * 0.3;

    color = mix(color, WAVEFORM_GLOW, glow_intensity);
    color = mix(color, WAVEFORM_COLOR, line_intensity);

    return vec4<f32>(color, 1.0);
}
