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

@binding(1) @group(0) var<storage, read_write> out_0 : array<Particle_std430_0>;

@binding(0) @group(0) var<storage, read> input_0 : array<Particle_std430_0>;

@compute
@workgroup_size(64, 1, 1)
fn gpu_particles_snapshot(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&out_0), 28);
    var i_0 : u32 = id_0.x;
    if(i_0 < (_S1.x))
    {
        out_0[i_0] = input_0[i_0];
    }
    return;
}

