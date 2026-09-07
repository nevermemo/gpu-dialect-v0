struct Settings_std430_0
{
    @align(4) scale_0 : f32,
    @align(4) offset_0 : f32,
    @align(4) threshold_0 : f32,
    @align(4) block_0 : u32,
};

@binding(1) @group(0) var<storage, read> settings_0 : array<Settings_std430_0>;

@binding(2) @group(0) var<storage, read_write> intermediate_0 : array<f32>;

@binding(0) @group(0) var<storage, read> samples_0 : array<f32>;

@compute
@workgroup_size(64, 1, 1)
fn gpu_staged_transform(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&settings_0), 16);
    var _gpu_len_settings_0 : u32 = _S1.x;
    var _S2 : vec2<u32> = vec2<u32>(arrayLength(&intermediate_0), 4);
    var i_0 : u32 = id_0.x;
    var _S3 : bool;
    if(i_0 < (_S2.x))
    {
        _S3 = u32(0) < _gpu_len_settings_0;
    }
    else
    {
        _S3 = false;
    }
    if(_S3)
    {
        var parameters_0 : Settings_std430_0 = settings_0[u32(0)];
        intermediate_0[i_0] = samples_0[i_0] * parameters_0.scale_0 + parameters_0.offset_0;
    }
    return;
}

