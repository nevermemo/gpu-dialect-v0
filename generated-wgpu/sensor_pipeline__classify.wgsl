struct Alert_std430_0
{
    @align(4) score_0 : f32,
    @align(4) quality_0 : f32,
    @align(4) sensor_id_0 : u32,
    @align(4) severity_0 : i32,
};

@binding(2) @group(0) var<storage, read_write> out_0 : array<Alert_std430_0>;

struct CalibratedSample_std430_0
{
    @align(4) temperature_0 : f32,
    @align(4) humidity_0 : f32,
    @align(4) quality_1 : f32,
    @align(4) sensor_id_1 : u32,
};

@binding(0) @group(0) var<storage, read> samples_0 : array<CalibratedSample_std430_0>;

struct AlertThresholds_std430_0
{
    @align(4) target_temperature_0 : f32,
    @align(4) target_humidity_0 : f32,
    @align(4) temperature_weight_0 : f32,
    @align(4) humidity_weight_0 : f32,
    @align(4) warning_0 : f32,
    @align(4) critical_0 : f32,
};

@binding(1) @group(0) var<storage, read> thresholds_0 : array<AlertThresholds_std430_0>;

fn classification_score_0( sample_0 : ptr<function, CalibratedSample_std430_0>,  thresholds_1 : ptr<function, AlertThresholds_std430_0>) -> f32
{
    var temperature_delta_0 : f32 = (*sample_0).temperature_0 - (*thresholds_1).target_temperature_0;
    var humidity_delta_0 : f32 = (*sample_0).humidity_0 - (*thresholds_1).target_humidity_0;
    return temperature_delta_0 * temperature_delta_0 * (*thresholds_1).temperature_weight_0 + humidity_delta_0 * humidity_delta_0 * (*thresholds_1).humidity_weight_0;
}

@compute
@workgroup_size(64, 1, 1)
fn gpu_sensor_pipeline_classify(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&out_0), 16);
    var i_0 : u32 = id_0.x;
    if(i_0 < (_S1.x))
    {
        var _S2 : CalibratedSample_std430_0 = samples_0[i_0];
        var _S3 : AlertThresholds_std430_0 = thresholds_0[i_0];
        var _S4 : f32 = classification_score_0(&(_S2), &(_S3));
        out_0[i_0].score_0 = _S4;
        out_0[i_0].quality_0 = _S2.quality_1;
        out_0[i_0].sensor_id_0 = _S2.sensor_id_1;
        if(_S4 > (_S3.critical_0))
        {
            out_0[i_0].severity_0 = i32(2);
        }
        else
        {
            if(_S4 > (_S3.warning_0))
            {
                out_0[i_0].severity_0 = i32(1);
            }
            else
            {
                out_0[i_0].severity_0 = i32(0);
            }
        }
    }
    return;
}

