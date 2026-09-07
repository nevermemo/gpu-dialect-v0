struct Pair_std430_0
{
    @align(4) x_0 : f32,
    @align(4) y_0 : f32,
};

struct Particle_std430_0
{
    @align(4) position_0 : Pair_std430_0,
    @align(4) velocity_0 : Pair_std430_0,
    @align(4) mass_0 : f32,
    @align(4) tag_0 : u32,
    @align(4) charge_0 : i32,
};

@binding(2) @group(0) var<storage, read_write> out_0 : array<Particle_std430_0>;

@binding(3) @group(0) var<storage, read_write> energy_0 : array<f32>;

@binding(0) @group(0) var<storage, read> input_0 : array<Particle_std430_0>;

@binding(1) @group(0) var<storage, read> acceleration_0 : array<Pair_std430_0>;

@compute
@workgroup_size(64, 1, 1)
fn gpu_particles_step(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&out_0), 28);
    var _gpu_len_out_0 : u32 = _S1.x;
    var _S2 : vec2<u32> = vec2<u32>(arrayLength(&energy_0), 4);
    var i_0 : u32 = id_0.x;
    if(i_0 < _gpu_len_out_0)
    {
        var particle_0 : Particle_std430_0 = input_0[i_0];
        var force_0 : Pair_std430_0 = acceleration_0[i_0];
        var vx_0 : f32 = particle_0.velocity_0.x_0 + force_0.x_0 * 0.01600000075995922f;
        var vy_0 : f32 = particle_0.velocity_0.y_0 + force_0.y_0 * 0.01600000075995922f;
        out_0[i_0] = particle_0;
        out_0[i_0].velocity_0.x_0 = vx_0;
        out_0[i_0].velocity_0.y_0 = vy_0;
        out_0[i_0].position_0.x_0 = particle_0.position_0.x_0 + vx_0 * 0.01600000075995922f;
        out_0[i_0].position_0.y_0 = particle_0.position_0.y_0 + vy_0 * 0.01600000075995922f;
        energy_0[i_0] = (out_0[i_0].velocity_0.x_0 * out_0[i_0].velocity_0.x_0 + out_0[i_0].velocity_0.y_0 * out_0[i_0].velocity_0.y_0) * particle_0.mass_0 * 0.5f;
    }
    return;
}

