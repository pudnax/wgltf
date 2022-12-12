struct Uniform {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
};

struct Camera {
	view_pos: vec3<f32>,
	proj_view: mat4x4<f32>,
	inv_proj: mat4x4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) vert_pos: vec3<f32>
}

@group(0) @binding(0)
var<uniform> un: Uniform;
@group(1) @binding(0)
var<uniform> cam: Camera;

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    let x = (1. - f32(in_vertex_index)) * 0.5;
    let y = f32(in_vertex_index & 1u) - 0.5;
    let clip_position = cam.proj_view * vec4(x, y, 0.0, 1.0);
    let vert_pos = clip_position.xyz;
    return VertexOutput(clip_position, vert_pos);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4(0.5, 0.4 + smoothstep(0., 0.5, in.vert_pos.y), 0.4 + fract(un.time) * 0.25, 1.);
}
