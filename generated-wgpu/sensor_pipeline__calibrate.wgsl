struct CalibratedSample_std430_0
{
    @align(4) temperature_0 : f32,
    @align(4) humidity_0 : f32,
    @align(4) quality_0 : f32,
    @align(4) sensor_id_0 : u32,
};

@binding(2) @group(0) var<storage, read_write> out_0 : array<CalibratedSample_std430_0>;

struct SensorReading_std430_0
{
    @align(4) raw_temperature_0 : f32,
    @align(4) raw_humidity_0 : f32,
    @align(4) sensor_id_1 : u32,
    @align(4) status_0 : i32,
};

@binding(0) @group(0) var<storage, read> readings_0 : array<SensorReading_std430_0>;

struct Calibration_std430_0
{
    @align(4) temperature_scale_0 : f32,
    @align(4) temperature_bias_0 : f32,
    @align(4) humidity_scale_0 : f32,
    @align(4) humidity_bias_0 : f32,
};

@binding(1) @group(0) var<storage, read> calibration_0 : array<Calibration_std430_0>;

fn clamp_value_0( value_0 : f32,  low_0 : f32,  high_0 : f32) -> f32
{
    var result_0 : f32;
    if(value_0 < low_0)
    {
        result_0 = low_0;
    }
    else
    {
        result_0 = value_0;
    }
    if(result_0 > high_0)
    {
        result_0 = high_0;
    }
    else
    {
    }
    return result_0;
}

fn sample_quality_0( humidity_1 : f32) -> f32
{
    var centered_0 : f32 = humidity_1 - 0.5f;
    return clamp_value_0(1.0f - centered_0 * centered_0, 0.0f, 1.0f);
}

@compute
@workgroup_size(64, 1, 1)
fn gpu_sensor_pipeline_calibrate(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S1 : vec2<u32> = vec2<u32>(arrayLength(&out_0), 16);
    var i_0 : u32 = id_0.x;
    if(i_0 < (_S1.x))
    {
        var reading_0 : SensorReading_std430_0 = readings_0[i_0];
        var parameters_0 : Calibration_std430_0 = calibration_0[i_0];
        var humidity_2 : f32 = clamp_value_0(reading_0.raw_humidity_0 * parameters_0.humidity_scale_0 + parameters_0.humidity_bias_0, 0.0f, 1.0f);
        out_0[i_0].temperature_0 = reading_0.raw_temperature_0 * parameters_0.temperature_scale_0 + parameters_0.temperature_bias_0;
        out_0[i_0].humidity_0 = humidity_2;
        out_0[i_0].quality_0 = sample_quality_0(humidity_2);
        out_0[i_0].sensor_id_0 = reading_0.sensor_id_1;
    }
    return;
}

