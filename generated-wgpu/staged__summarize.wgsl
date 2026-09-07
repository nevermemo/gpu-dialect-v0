@binding(0) @group(0) var<storage, read> intermediate_0 : array<f32>;

struct Settings_std430_0
{
    @align(4) scale_0 : f32,
    @align(4) offset_0 : f32,
    @align(4) threshold_0 : f32,
    @align(4) block_0 : u32,
};

@binding(1) @group(0) var<storage, read> settings_0 : array<Settings_std430_0>;

struct Summary_std430_0
{
    @align(4) total_0 : f32,
    @align(4) peak_0 : f32,
    @align(4) above_threshold_0 : u32,
    @align(4) count_0 : u32,
};

@binding(2) @group(0) var<storage, read_write> summaries_0 : array<Summary_std430_0>;

struct Summary_0
{
     total_0 : f32,
     peak_0 : f32,
     above_threshold_0 : u32,
     count_0 : u32,
};

fn fold_0( acc_0 : Summary_0,  value_0 : f32,  threshold_1 : f32) -> Summary_0
{
    var _gust_struct_0_0 : Summary_0;
    _gust_struct_0_0.total_0 = acc_0.total_0 + value_0;
    _gust_struct_0_0.peak_0 = acc_0.peak_0;
    _gust_struct_0_0.above_threshold_0 = acc_0.above_threshold_0;
    _gust_struct_0_0.count_0 = acc_0.count_0 + u32(1);
    var next_0 : Summary_0 = _gust_struct_0_0;
    if(value_0 > (next_0.peak_0))
    {
        next_0.peak_0 = value_0;
    }
    if(value_0 > threshold_1)
    {
        next_0.above_threshold_0 = next_0.above_threshold_0 + u32(1);
    }
    return next_0;
}

@compute
@workgroup_size(64, 1, 1)
fn gpu_staged_summarize(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&intermediate_0), 4);
    var _gpu_len_intermediate_0 : u32 = _S1.x;
    var _S2 : vec2<u32> = vec2<u32>(arrayLength(&settings_0), 16);
    var _gpu_len_settings_0 : u32 = _S2.x;
    var _S3 : vec2<u32> = vec2<u32>(arrayLength(&summaries_0), 16);
    var b_0 : u32 = id_0.x;
    var _S4 : bool;
    if(b_0 < (_S3.x))
    {
        _S4 = u32(0) < _gpu_len_settings_0;
    }
    else
    {
        _S4 = false;
    }
    if(_S4)
    {
        var parameters_0 : Settings_std430_0 = settings_0[u32(0)];
        var start_0 : u32 = b_0 * parameters_0.block_0;
        var _gust_struct_3_0 : Summary_0;
        _gust_struct_3_0.total_0 = 0.0f;
        _gust_struct_3_0.peak_0 = 0.0f;
        _gust_struct_3_0.above_threshold_0 = u32(0);
        _gust_struct_3_0.count_0 = u32(0);
        var acc_1 : Summary_0 = _gust_struct_3_0;
        var acc_2 : Summary_0;
        if(start_0 < _gpu_len_intermediate_0)
        {
            acc_2 = fold_0(acc_1, intermediate_0[start_0], parameters_0.threshold_0);
        }
        else
        {
            acc_2 = acc_1;
        }
        var _S5 : u32 = start_0 + u32(1);
        if(_S5 < _gpu_len_intermediate_0)
        {
            acc_2 = fold_0(acc_2, intermediate_0[_S5], parameters_0.threshold_0);
        }
        else
        {
        }
        var _S6 : u32 = start_0 + u32(2);
        if(_S6 < _gpu_len_intermediate_0)
        {
            acc_2 = fold_0(acc_2, intermediate_0[_S6], parameters_0.threshold_0);
        }
        else
        {
        }
        var _S7 : u32 = start_0 + u32(3);
        if(_S7 < _gpu_len_intermediate_0)
        {
            acc_2 = fold_0(acc_2, intermediate_0[_S7], parameters_0.threshold_0);
        }
        else
        {
        }
        var _S8 : u32 = start_0 + u32(4);
        if(_S8 < _gpu_len_intermediate_0)
        {
            acc_2 = fold_0(acc_2, intermediate_0[_S8], parameters_0.threshold_0);
        }
        else
        {
        }
        var _S9 : u32 = start_0 + u32(5);
        if(_S9 < _gpu_len_intermediate_0)
        {
            acc_2 = fold_0(acc_2, intermediate_0[_S9], parameters_0.threshold_0);
        }
        else
        {
        }
        var _S10 : u32 = start_0 + u32(6);
        if(_S10 < _gpu_len_intermediate_0)
        {
            acc_2 = fold_0(acc_2, intermediate_0[_S10], parameters_0.threshold_0);
        }
        else
        {
        }
        var _S11 : u32 = start_0 + u32(7);
        if(_S11 < _gpu_len_intermediate_0)
        {
            acc_2 = fold_0(acc_2, intermediate_0[_S11], parameters_0.threshold_0);
        }
        else
        {
        }
        summaries_0[b_0].total_0 = acc_2.total_0;
        summaries_0[b_0].peak_0 = acc_2.peak_0;
        summaries_0[b_0].above_threshold_0 = acc_2.above_threshold_0;
        summaries_0[b_0].count_0 = acc_2.count_0;
    }
    return;
}

