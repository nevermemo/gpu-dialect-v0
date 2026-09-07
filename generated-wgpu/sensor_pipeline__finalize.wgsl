struct Report_std430_0
{
    @align(4) adjusted_score_0 : f32,
    @align(4) confidence_0 : f32,
    @align(4) sensor_id_0 : u32,
    @align(4) action_0 : i32,
};

@binding(1) @group(0) var<storage, read_write> out_0 : array<Report_std430_0>;

struct Alert_std430_0
{
    @align(4) score_0 : f32,
    @align(4) quality_0 : f32,
    @align(4) sensor_id_1 : u32,
    @align(4) severity_0 : i32,
};

@binding(0) @group(0) var<storage, read> alerts_0 : array<Alert_std430_0>;

@compute
@workgroup_size(64, 1, 1)
fn gpu_sensor_pipeline_finalize(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&out_0), 16);
    var i_0 : u32 = id_0.x;
    if(i_0 < (_S1.x))
    {
        var alert_0 : Alert_std430_0 = alerts_0[i_0];
        out_0[i_0].adjusted_score_0 = alert_0.score_0 + (1.0f - alert_0.quality_0) * 0.5f;
        out_0[i_0].confidence_0 = alert_0.quality_0;
        out_0[i_0].sensor_id_0 = alert_0.sensor_id_1;
        var _S2 : bool;
        if((alert_0.severity_0) > i32(0))
        {
            _S2 = (alert_0.quality_0) < 0.25f;
        }
        else
        {
            _S2 = false;
        }
        if(_S2)
        {
            out_0[i_0].action_0 = i32(-1);
        }
        else
        {
            out_0[i_0].action_0 = alert_0.severity_0;
        }
    }
    return;
}

