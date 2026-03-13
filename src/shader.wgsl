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
    time_val: f32,
    num_channels: u32,
    is_dark: u32,
    visible_samples: u32,
    texture_width: u32,
    texture_height: u32,
};

var<immediate> params: PlotParams;

@group(0) @binding(0) var<storage, read> samples: array<f32>;

fn get_sample(channel: u32, index: u32) -> f32 {
    let start = (params.write_pos + params.num_samples - params.visible_samples) % params.num_samples;
    let actual_index = (start + index) % params.num_samples;
    return samples[actual_index * params.num_channels + channel];
}

fn value_to_y(value: f32) -> f32 {
    return (value - params.y_min) / (params.y_max - params.y_min);
}

fn channel_color(ch: u32) -> vec3<f32> {
    if ch == 0u {
        return vec3<f32>(1.0, 0.0, 1.0);   // magenta - Phase 1
    } else if ch == 1u {
        return vec3<f32>(1.0, 0.2, 0.2);   // red - Phase 2
    } else {
        return vec3<f32>(0.0, 0.8, 0.0);   // green - Phase 3
    }
}

fn channel_glow(ch: u32) -> vec3<f32> {
    if params.is_dark == 0u {
        return channel_color(ch);
    }
    if ch == 0u {
        return vec3<f32>(0.4, 0.0, 0.4);
    } else if ch == 1u {
        return vec3<f32>(0.4, 0.08, 0.08);
    } else {
        return vec3<f32>(0.0, 0.35, 0.0);
    }
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    var color = vec3<f32>(0.0);
    var alpha = 0.0;

    // Screen-pixel sizes in UV space
    let px_x = 1.0 / f32(params.texture_width);
    let px_y = 1.0 / f32(params.texture_height);

    // Waveforms — iterate over channels
    let vis = params.visible_samples;
    let sample_x = uv.x * f32(vis - 1u);
    let sample_index_low = u32(floor(sample_x));
    let sample_index_high = min(sample_index_low + 1u, vis - 1u);
    let frac = fract(sample_x);

    for (var ch = 0u; ch < params.num_channels; ch++) {
        let val_low = get_sample(ch, sample_index_low);
        let val_high = get_sample(ch, sample_index_high);
        let val = mix(val_low, val_high, frac);

        let waveform_y = 1.0 - value_to_y(val);
        let dist_to_waveform = abs(uv.y - waveform_y);

        // Fill vertical segments for steep transitions
        let y_low = 1.0 - value_to_y(val_low);
        let y_high = 1.0 - value_to_y(val_high);
        let y_min_seg = min(y_low, y_high);
        let y_max_seg = max(y_low, y_high);

        var segment_dist = dist_to_waveform;
        if uv.y >= y_min_seg && uv.y <= y_max_seg {
            let nearest_sample_x = round(sample_x) / f32(vis - 1u);
            segment_dist = min(segment_dist, abs(uv.x - nearest_sample_x) * f32(vis));
        }

        // Line width in screen pixels (constant regardless of zoom)
        let line_intensity = smoothstep(px_y * 2.0, 0.0, segment_dist);

        var glow_intensity = smoothstep(px_y * 6.0, 0.0, segment_dist) * 0.25;
        if params.is_dark == 0u {
            glow_intensity = 0.0;
        }

        color = mix(color, channel_glow(ch), glow_intensity);
        alpha = max(alpha, glow_intensity);
        color = mix(color, channel_color(ch), line_intensity);
        alpha = max(alpha, line_intensity);
    }

    return vec4<f32>(color, alpha);
}
