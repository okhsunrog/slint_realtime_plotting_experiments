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

    let px_y = 1.0 / f32(params.texture_height);
    let vis = params.visible_samples;

    // How many samples fall within one pixel column
    let samples_per_pixel = f32(vis) / f32(params.texture_width);
    let sample_center = uv.x * f32(vis - 1u);

    // Sample range for this pixel column (at least 1 sample on each side for continuity)
    let half_span = max(samples_per_pixel * 0.5, 0.5);
    let s_start = u32(clamp(floor(sample_center - half_span), 0.0, f32(vis - 1u)));
    let s_end = u32(clamp(ceil(sample_center + half_span), 0.0, f32(vis - 1u)));
    let iter_count = min(s_end - s_start + 1u, 256u);

    for (var ch = 0u; ch < params.num_channels; ch++) {
        // Find min/max Y values across all samples in this pixel column
        var val_min = get_sample(ch, s_start);
        var val_max = val_min;

        for (var i = 1u; i < iter_count; i++) {
            let val = get_sample(ch, s_start + i);
            val_min = min(val_min, val);
            val_max = max(val_max, val);
        }

        // Convert to screen Y (inverted: higher value = lower screen Y)
        let y_top = 1.0 - value_to_y(val_max);
        let y_bot = 1.0 - value_to_y(val_min);

        // Distance from pixel to the vertical line segment [y_top, y_bot]
        var dist: f32;
        if uv.y < y_top {
            dist = y_top - uv.y;
        } else if uv.y > y_bot {
            dist = uv.y - y_bot;
        } else {
            dist = 0.0;
        }

        let line_intensity = smoothstep(px_y * 2.0, 0.0, dist);

        var glow_intensity = smoothstep(px_y * 6.0, 0.0, dist) * 0.25;
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
