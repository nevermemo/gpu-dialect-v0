struct Active_std430_0
{
    @align(4) count_0 : u32,
};

@binding(0) @group(0) var<storage, read> active_0 : array<Active_std430_0>;

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

@binding(2) @group(0) var<storage, read_write> particles_0 : array<Particle_std430_0>;

struct Particle_0
{
     x_0 : f32,
     y_0 : f32,
     vx_0 : f32,
     vy_0 : f32,
     steps_0 : u32,
};

@compute
@workgroup_size(64, 1, 1)
fn gpu_pool_integrate(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&active_0), 4);
    var _gpu_len_active_0 : u32 = _S1.x;
    var _S2 : vec2<u32> = vec2<u32>(arrayLength(&settings_0), 12);
    var _gpu_len_settings_0 : u32 = _S2.x;
    var _S3 : vec2<u32> = vec2<u32>(arrayLength(&particles_0), 20);
    var _gpu_len_particles_0 : u32 = _S3.x;
    var i_0 : u32 = id_0.x;
    var _S4 : bool;
    if(u32(0) < _gpu_len_active_0)
    {
        _S4 = u32(0) < _gpu_len_settings_0;
    }
    else
    {
        _S4 = false;
    }
    if(_S4)
    {
        _S4 = i_0 < (active_0[u32(0)].count_0);
    }
    else
    {
        _S4 = false;
    }
    if(_S4)
    {
        _S4 = i_0 < _gpu_len_particles_0;
    }
    else
    {
        _S4 = false;
    }
    if(_S4)
    {
        var s_0 : Settings_std430_0 = settings_0[u32(0)];
        var p_0 : Particle_0;
        p_0.x_0 = particles_0[i_0].x_0;
        p_0.y_0 = particles_0[i_0].y_0;
        p_0.vx_0 = particles_0[i_0].vx_0;
        p_0.vy_0 = particles_0[i_0].vy_0;
        p_0.steps_0 = particles_0[i_0].steps_0;
        var _S5 : f32 = p_0.vy_0 + s_0.gravity_0 * s_0.dt_0;
        p_0.vy_0 = _S5;
        p_0.x_0 = p_0.x_0 + p_0.vx_0 * s_0.dt_0;
        p_0.y_0 = p_0.y_0 + _S5 * s_0.dt_0;
        p_0.steps_0 = p_0.steps_0 + u32(1);
        particles_0[i_0].x_0 = p_0.x_0;
        particles_0[i_0].y_0 = p_0.y_0;
        particles_0[i_0].vx_0 = p_0.vx_0;
        particles_0[i_0].vy_0 = p_0.vy_0;
        particles_0[i_0].steps_0 = p_0.steps_0;
    }
    return;
}

