@binding(0) @group(0) var<storage, read> count_0 : array<u32>;

struct Settings_std430_0
{
    @align(4) dt_0 : f32,
    @align(4) gravity_0 : f32,
    @align(4) budget_0 : u32,
};

@binding(1) @group(0) var<storage, read> settings_0 : array<Settings_std430_0>;

struct Particle_std430_0
{
    @align(4) x_0 : f32,
    @align(4) y_0 : f32,
    @align(4) vx_0 : f32,
    @align(4) vy_0 : f32,
    @align(4) steps_0 : u32,
};

@binding(2) @group(0) var<storage, read> particles_0 : array<Particle_std430_0>;

struct Active_std430_0
{
    @align(4) count_1 : u32,
};

@binding(3) @group(0) var<storage, read_write> active_0 : array<Active_std430_0>;

struct DispatchArgs_std430_0
{
    @align(4) x_1 : u32,
    @align(4) y_1 : u32,
    @align(4) z_0 : u32,
};

@binding(4) @group(0) var<storage, read_write> args_0 : array<DispatchArgs_std430_0>;

fn min_uint_0( a_0 : u32,  b_0 : u32) -> u32
{
    if(a_0 < b_0)
    {
        return a_0;
    }
    else
    {
        return b_0;
    }
}

@compute
@workgroup_size(1, 1, 1)
fn gpu_pool_prepare_dispatch(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&count_0), 4);
    var _gpu_len_count_0 : u32 = _S1.x;
    var _S2 : vec2<u32> = vec2<u32>(arrayLength(&settings_0), 12);
    var _gpu_len_settings_0 : u32 = _S2.x;
    var _S3 : vec2<u32> = vec2<u32>(arrayLength(&particles_0), 20);
    var _gpu_len_particles_0 : u32 = _S3.x;
    var _S4 : vec2<u32> = vec2<u32>(arrayLength(&active_0), 4);
    var _S5 : vec2<u32> = vec2<u32>(arrayLength(&args_0), 12);
    var _S6 : bool;
    if((id_0.x) == u32(0))
    {
        _S6 = u32(0) < _gpu_len_count_0;
    }
    else
    {
        _S6 = false;
    }
    if(_S6)
    {
        _S6 = u32(0) < _gpu_len_settings_0;
    }
    else
    {
        _S6 = false;
    }
    if(_S6)
    {
        var n_0 : u32 = min_uint_0(min_uint_0(count_0[u32(0)], _gpu_len_particles_0), settings_0[u32(0)].budget_0);
        active_0[u32(0)].count_1 = n_0;
        args_0[u32(0)].x_1 = n_0 / u32(64) + min_uint_0(n_0 % u32(64), u32(1));
        args_0[u32(0)].y_1 = u32(1);
        args_0[u32(0)].z_0 = u32(1);
    }
    return;
}

