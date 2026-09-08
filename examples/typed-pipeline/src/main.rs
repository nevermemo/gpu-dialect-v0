use std::{path::PathBuf, time::Instant};

#[cfg(test)]
use gpu_dialect::GpuPod;
use gpu_dialect::{KernelDescriptor, gpu};
use gpu_dialect_wgpu::{
    BufferBinding, BufferDispatch, GpuBufferAccess, HeadlessDevice, PipelineCacheStats,
    render_wgpu_source,
};

#[gpu]
mod sensor_pipeline {
    #[derive(Debug, PartialEq)]
    pub struct SensorReading {
        pub raw_temperature: float,
        pub raw_humidity: float,
        pub sensor_id: uint,
        pub status: int,
    }

    #[derive(Debug, PartialEq)]
    pub struct Calibration {
        pub temperature_scale: float,
        pub temperature_bias: float,
        pub humidity_scale: float,
        pub humidity_bias: float,
    }

    #[derive(Debug, PartialEq)]
    pub struct CalibratedSample {
        pub temperature: float,
        pub humidity: float,
        pub quality: float,
        pub sensor_id: uint,
    }

    #[derive(Debug, PartialEq)]
    pub struct AlertThresholds {
        pub target_temperature: float,
        pub target_humidity: float,
        pub temperature_weight: float,
        pub humidity_weight: float,
        pub warning: float,
        pub critical: float,
    }

    #[derive(Debug, PartialEq)]
    pub struct Alert {
        pub score: float,
        pub quality: float,
        pub sensor_id: uint,
        pub severity: int,
    }

    #[derive(Debug, PartialEq)]
    pub struct Report {
        pub adjusted_score: float,
        pub confidence: float,
        pub sensor_id: uint,
        pub action: int,
    }

    fn clamp_value(value: float, low: float, high: float) -> float {
        let mut result = value;
        if result < low {
            result = low;
        }
        if result > high {
            result = high;
        }
        result
    }

    fn sample_quality(humidity: float) -> float {
        let centered = humidity - 0.5;
        clamp_value(1.0 - centered * centered, 0.0, 1.0)
    }

    fn classification_score(sample: CalibratedSample, thresholds: AlertThresholds) -> float {
        let temperature_delta = sample.temperature - thresholds.target_temperature;
        let humidity_delta = sample.humidity - thresholds.target_humidity;
        let temperature_score =
            temperature_delta * temperature_delta * thresholds.temperature_weight;
        let humidity_score = humidity_delta * humidity_delta * thresholds.humidity_weight;
        temperature_score + humidity_score
    }

    #[kernel(workgroup_size(64, 1, 1))]
    pub fn calibrate(
        id: SV_DispatchThreadID,
        readings: StructuredBuffer<SensorReading>,
        calibration: StructuredBuffer<Calibration>,
        mut out: RWStructuredBuffer<CalibratedSample>,
    ) {
        let i = id.x;
        if i < out.len() {
            let reading = readings[i];
            let parameters = calibration[i];
            let temperature = reading.raw_temperature * parameters.temperature_scale
                + parameters.temperature_bias;
            let humidity = clamp_value(
                reading.raw_humidity * parameters.humidity_scale + parameters.humidity_bias,
                0.0,
                1.0,
            );
            out[i].temperature = temperature;
            out[i].humidity = humidity;
            out[i].quality = sample_quality(humidity);
            out[i].sensor_id = reading.sensor_id;
        }
    }

    #[kernel(workgroup_size(64, 1, 1))]
    pub fn classify(
        id: SV_DispatchThreadID,
        samples: StructuredBuffer<CalibratedSample>,
        thresholds: StructuredBuffer<AlertThresholds>,
        mut out: RWStructuredBuffer<Alert>,
    ) {
        let i = id.x;
        if i < out.len() {
            let sample = samples[i];
            let limits = thresholds[i];
            let score = classification_score(sample, limits);
            out[i].score = score;
            out[i].quality = sample.quality;
            out[i].sensor_id = sample.sensor_id;
            if score > limits.critical {
                out[i].severity = 2;
            } else if score > limits.warning {
                out[i].severity = 1;
            } else {
                out[i].severity = 0;
            }
        }
    }

    #[kernel(workgroup_size(64, 1, 1))]
    pub fn finalize(
        id: SV_DispatchThreadID,
        alerts: StructuredBuffer<Alert>,
        mut out: RWStructuredBuffer<Report>,
    ) {
        let i = id.x;
        if i < out.len() {
            let alert = alerts[i];
            let quality_penalty = (1.0 - alert.quality) * 0.5;
            out[i].adjusted_score = alert.score + quality_penalty;
            out[i].confidence = alert.quality;
            out[i].sensor_id = alert.sensor_id;
            if alert.severity > 0 && alert.quality < 0.25 {
                out[i].action = -1;
            } else {
                out[i].action = alert.severity;
            }
        }
    }
}

struct Inputs {
    readings: Vec<sensor_pipeline::SensorReading>,
    calibration: Vec<sensor_pipeline::Calibration>,
    thresholds: Vec<sensor_pipeline::AlertThresholds>,
}

struct PipelineOutput {
    samples: Vec<sensor_pipeline::CalibratedSample>,
    alerts: Vec<sensor_pipeline::Alert>,
    reports: Vec<sensor_pipeline::Report>,
    cache: PipelineCacheStats,
}

fn make_inputs(count: usize) -> Inputs {
    let indices = 0..count;
    Inputs {
        readings: indices
            .clone()
            .map(|index| sensor_pipeline::SensorReading {
                raw_temperature: 18.0 + (index % 29) as f32 * 0.35,
                raw_humidity: 0.2 + (index % 41) as f32 * 0.015,
                sensor_id: 10_000 + index as u32,
                status: (index % 5) as i32 - 2,
            })
            .collect(),
        calibration: indices
            .clone()
            .map(|index| sensor_pipeline::Calibration {
                temperature_scale: 0.98 + (index % 3) as f32 * 0.01,
                temperature_bias: (index % 7) as f32 * 0.05 - 0.15,
                humidity_scale: 0.95 + (index % 5) as f32 * 0.02,
                humidity_bias: (index % 11) as f32 * 0.005 - 0.025,
            })
            .collect(),
        thresholds: indices
            .map(|index| sensor_pipeline::AlertThresholds {
                target_temperature: 22.0,
                target_humidity: 0.5,
                temperature_weight: 0.08 + (index % 2) as f32 * 0.01,
                humidity_weight: 2.0,
                warning: 0.8,
                critical: 2.4,
            })
            .collect(),
    }
}

fn clamp_value(value: f32, low: f32, high: f32) -> f32 {
    value.max(low).min(high)
}

fn run_cpu(inputs: &Inputs) -> PipelineOutput {
    let mut samples = Vec::with_capacity(inputs.readings.len());
    let mut alerts = Vec::with_capacity(inputs.readings.len());
    let mut reports = Vec::with_capacity(inputs.readings.len());

    for ((reading, calibration), thresholds) in inputs
        .readings
        .iter()
        .zip(&inputs.calibration)
        .zip(&inputs.thresholds)
    {
        let temperature =
            reading.raw_temperature * calibration.temperature_scale + calibration.temperature_bias;
        let humidity = clamp_value(
            reading.raw_humidity * calibration.humidity_scale + calibration.humidity_bias,
            0.0,
            1.0,
        );
        let centered = humidity - 0.5;
        let quality = clamp_value(1.0 - centered * centered, 0.0, 1.0);
        let sample = sensor_pipeline::CalibratedSample {
            temperature,
            humidity,
            quality,
            sensor_id: reading.sensor_id,
        };

        let temperature_delta = sample.temperature - thresholds.target_temperature;
        let humidity_delta = sample.humidity - thresholds.target_humidity;
        let score = temperature_delta * temperature_delta * thresholds.temperature_weight
            + humidity_delta * humidity_delta * thresholds.humidity_weight;
        let severity = if score > thresholds.critical {
            2
        } else if score > thresholds.warning {
            1
        } else {
            0
        };
        let alert = sensor_pipeline::Alert {
            score,
            quality: sample.quality,
            sensor_id: sample.sensor_id,
            severity,
        };
        let report = sensor_pipeline::Report {
            adjusted_score: alert.score + (1.0 - alert.quality) * 0.5,
            confidence: alert.quality,
            sensor_id: alert.sensor_id,
            action: if alert.severity > 0 && alert.quality < 0.25 {
                -1
            } else {
                alert.severity
            },
        };
        samples.push(sample);
        alerts.push(alert);
        reports.push(report);
    }

    PipelineOutput {
        samples,
        alerts,
        reports,
        cache: PipelineCacheStats::default(),
    }
}

fn run_gpu(
    device: &HeadlessDevice,
    inputs: &Inputs,
) -> Result<PipelineOutput, gpu_dialect_wgpu::Error> {
    let count = inputs.readings.len();
    let readings = device.create_typed_buffer(
        "sensor readings",
        &inputs.readings,
        GpuBufferAccess::ReadOnly,
    );
    let calibration = device.create_typed_buffer(
        "sensor calibration",
        &inputs.calibration,
        GpuBufferAccess::ReadOnly,
    );
    let thresholds = device.create_typed_buffer(
        "alert thresholds",
        &inputs.thresholds,
        GpuBufferAccess::ReadOnly,
    );
    let sample_seed = vec![empty_sample(); count];
    let alert_seed = vec![empty_alert(); count];
    let report_seed = vec![empty_report(); count];
    let samples = device.create_typed_buffer(
        "calibrated samples",
        &sample_seed,
        GpuBufferAccess::ReadWrite,
    );
    let alerts = device.create_typed_buffer("alerts", &alert_seed, GpuBufferAccess::ReadWrite);
    let reports = device.create_typed_buffer("reports", &report_seed, GpuBufferAccess::ReadWrite);

    let calibrate_bindings = [
        BufferBinding::read_only(&readings),
        BufferBinding::read_only(&calibration),
        BufferBinding::read_write(&samples),
    ];
    let classify_bindings = [
        BufferBinding::read_only(&samples),
        BufferBinding::read_only(&thresholds),
        BufferBinding::read_write(&alerts),
    ];
    let finalize_bindings = [
        BufferBinding::read_only(&alerts),
        BufferBinding::read_write(&reports),
    ];
    let dispatches = [
        BufferDispatch::new(
            &sensor_pipeline::calibrate::DESCRIPTOR,
            count as u32,
            &calibrate_bindings,
        ),
        BufferDispatch::new(
            &sensor_pipeline::classify::DESCRIPTOR,
            count as u32,
            &classify_bindings,
        ),
        BufferDispatch::new(
            &sensor_pipeline::finalize::DESCRIPTOR,
            count as u32,
            &finalize_bindings,
        ),
    ];
    device.dispatch_batch(&dispatches)?;

    Ok(PipelineOutput {
        samples: device.read_typed_buffer(&samples)?,
        alerts: device.read_typed_buffer(&alerts)?,
        reports: device.read_typed_buffer(&reports)?,
        cache: device.pipeline_cache_stats(),
    })
}

fn empty_sample() -> sensor_pipeline::CalibratedSample {
    sensor_pipeline::CalibratedSample {
        temperature: 0.0,
        humidity: 0.0,
        quality: 0.0,
        sensor_id: 0,
    }
}

fn empty_alert() -> sensor_pipeline::Alert {
    sensor_pipeline::Alert {
        score: 0.0,
        quality: 0.0,
        sensor_id: 0,
        severity: 0,
    }
}

fn empty_report() -> sensor_pipeline::Report {
    sensor_pipeline::Report {
        adjusted_score: 0.0,
        confidence: 0.0,
        sensor_id: 0,
        action: 0,
    }
}

fn assert_close(actual: &PipelineOutput, expected: &PipelineOutput) {
    assert_eq!(actual.samples.len(), expected.samples.len());
    assert_eq!(actual.alerts.len(), expected.alerts.len());
    assert_eq!(actual.reports.len(), expected.reports.len());
    for (actual, expected) in actual.samples.iter().zip(&expected.samples) {
        assert_float(actual.temperature, expected.temperature);
        assert_float(actual.humidity, expected.humidity);
        assert_float(actual.quality, expected.quality);
        assert_eq!(actual.sensor_id, expected.sensor_id);
    }
    for (actual, expected) in actual.alerts.iter().zip(&expected.alerts) {
        assert_float(actual.score, expected.score);
        assert_float(actual.quality, expected.quality);
        assert_eq!(actual.sensor_id, expected.sensor_id);
        assert_eq!(actual.severity, expected.severity);
    }
    for (actual, expected) in actual.reports.iter().zip(&expected.reports) {
        assert_float(actual.adjusted_score, expected.adjusted_score);
        assert_float(actual.confidence, expected.confidence);
        assert_eq!(actual.sensor_id, expected.sensor_id);
        assert_eq!(actual.action, expected.action);
    }
}

fn assert_float(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 2.0e-5 * expected.abs().max(1.0),
        "GPU value {actual} differs from CPU value {expected}"
    );
}

fn descriptors() -> [&'static KernelDescriptor; 3] {
    [
        &sensor_pipeline::calibrate::DESCRIPTOR,
        &sensor_pipeline::classify::DESCRIPTOR,
        &sensor_pipeline::finalize::DESCRIPTOR,
    ]
}

fn export_artifacts() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../generated-wgpu");
    std::fs::create_dir_all(&directory)?;
    for descriptor in descriptors() {
        let stem = format!("{}__{}", descriptor.module, descriptor.name);
        std::fs::write(
            directory.join(format!("{stem}.slang")),
            descriptor.slang_source,
        )?;
        std::fs::write(
            directory.join(format!("{stem}.wgsl")),
            gpu_dialect::slang::compile_wgsl(descriptor)?,
        )?;
        std::fs::write(
            directory.join(format!("{stem}.spv")),
            gpu_dialect::spirv::words_as_le_bytes(&gpu_dialect::slang::compile_spirv(descriptor)?),
        )?;
        std::fs::write(
            directory.join(format!("{stem}.rs")),
            render_wgpu_source(descriptor)?,
        )?;
    }
    Ok(directory.canonicalize().unwrap_or(directory))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = if cfg!(debug_assertions) {
        1 << 18
    } else {
        1 << 22
    };
    let inputs = make_inputs(count);
    let cpu_started = Instant::now();
    let expected = run_cpu(&inputs);
    let cpu_elapsed = cpu_started.elapsed();

    let device = HeadlessDevice::new()?;
    let gpu_started = Instant::now();
    let actual = run_gpu(&device, &inputs)?;
    let gpu_elapsed = gpu_started.elapsed();
    assert_close(&actual, &expected);
    let artifacts = export_artifacts()?;

    println!("Typed sensor pipeline: {count} elements");
    println!("adapter: {}", device.adapter_info().name);
    println!("shader entry points: {}", descriptors().len());
    println!("ordered dispatches per batch: 3");
    println!("distinct GPU struct types: 6");
    println!("CPU reference: {cpu_elapsed:?}");
    println!("GPU end-to-end: {gpu_elapsed:?}");
    println!("pipeline cache: {:?}", actual.cache);
    println!("generated artifacts: {}", artifacts.display());
    println!("first report: {:?}", actual.reports.first());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn descriptors_cover_distinct_types_helpers_and_bindings() {
        assert_eq!(sensor_pipeline::MODULE_DESCRIPTOR.kernels.len(), 3);
        let calibrate = sensor_pipeline::calibrate::DESCRIPTOR;
        assert!(calibrate.slang_source.contains("struct SensorReading"));
        assert!(calibrate.slang_source.contains("struct Calibration"));
        assert!(calibrate.slang_source.contains("struct CalibratedSample"));
        assert!(calibrate.slang_source.contains("float clamp_value("));
        assert!(calibrate.slang_source.contains("float sample_quality("));
        assert_eq!(calibrate.parameters[1].binding.unwrap().binding, 0);
        assert_eq!(calibrate.parameters[3].binding.unwrap().binding, 2);

        let classify = sensor_pipeline::classify::DESCRIPTOR;
        assert!(classify.slang_source.contains("struct AlertThresholds"));
        assert!(classify.slang_source.contains("struct Alert"));
        assert!(
            classify
                .slang_source
                .contains("float classification_score(")
        );

        let finalize = sensor_pipeline::finalize::DESCRIPTOR;
        assert!(finalize.slang_source.contains("struct Report"));
        assert_eq!(finalize.parameters[2].binding.unwrap().binding, 1);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn every_stage_compiles_to_wgsl() {
        for descriptor in descriptors() {
            let wgsl = gpu_dialect::slang::compile_wgsl(descriptor).unwrap();
            assert!(wgsl.contains("@compute"));
            assert!(wgsl.contains(descriptor.entry_point));
        }
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn dependent_multi_dispatch_pipeline_matches_cpu() {
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        for count in [0, 1, 63, 64, 65, 257] {
            let inputs = make_inputs(count);
            let expected = run_cpu(&inputs);
            let actual = run_gpu(&device, &inputs).unwrap();
            assert_close(&actual, &expected);
        }
        let cache = device.pipeline_cache_stats();
        assert_eq!(cache.entries, 3);
        assert_eq!(cache.misses, 3);
        assert!(cache.hits >= 12);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn struct_layouts_are_distinct_and_storage_compatible() {
        let layouts = [
            <sensor_pipeline::SensorReading as GpuPod>::LAYOUT,
            <sensor_pipeline::Calibration as GpuPod>::LAYOUT,
            <sensor_pipeline::CalibratedSample as GpuPod>::LAYOUT,
            <sensor_pipeline::AlertThresholds as GpuPod>::LAYOUT,
            <sensor_pipeline::Alert as GpuPod>::LAYOUT,
            <sensor_pipeline::Report as GpuPod>::LAYOUT,
        ];
        assert_eq!(layouts[0].size, 16);
        assert_eq!(layouts[3].size, 24);
        assert!(layouts.into_iter().all(|layout| layout.is_storage_v1()));
    }
}
