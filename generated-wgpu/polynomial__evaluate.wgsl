@binding(1) @group(0) var<storage, read_write> out_0 : array<f32>;

@binding(0) @group(0) var<storage, read> input_0 : array<f32>;

@compute
@workgroup_size(128, 1, 1)
fn gpu_polynomial_evaluate(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&out_0), 4);
    var i_0 : u32 = id_0.x;
    if(i_0 < (_S1.x))
    {
        var value_0 : f32 = input_0[i_0];
        var squared_0 : f32 = value_0 * value_0;
        out_0[i_0] = squared_0 * value_0 - 2.0f * squared_0 + 0.5f * value_0 + 1.0f;
    }
    return;
}

