#![allow(dead_code)]

use std::ops::Range;

#[derive(Clone)]
pub struct Q8Block {
    pub scale: f32,
    pub qs: [i8; 32],
}

pub struct Q8Fixture {
    pub batch: usize,
    pub n_in: usize,
    pub n_out: usize,
    pub rows: Range<u32>,
    pub weight_blocks: Vec<Q8Block>,
    pub weight_bytes: Vec<u8>,
    pub input: Vec<f32>,
    pub expected: Vec<f32>,
}

pub fn q8_fixture(batch: usize, n_in: usize, n_out: usize, rows: Range<u32>) -> Q8Fixture {
    assert_eq!(n_in % 32, 0);
    assert!(rows.start < rows.end && rows.end as usize <= n_out);

    let blocks_per_row = n_in / 32;
    let mut weight_blocks = Vec::with_capacity(n_out * blocks_per_row);
    let mut weight_bytes = Vec::with_capacity(n_out * blocks_per_row * 34);
    for row in 0..n_out {
        for block in 0..blocks_per_row {
            let scale = 0.01 * (1 + (row + block) % 7) as f32;
            let qs =
                std::array::from_fn(|lane| ((row * 3 + block * 5 + lane) as i32 % 31 - 15) as i8);
            weight_bytes.extend_from_slice(&half::f16::from_f32(scale).to_bits().to_le_bytes());
            weight_bytes.extend(qs.iter().map(|value| *value as u8));
            weight_blocks.push(Q8Block {
                scale: half::f16::from_f32(scale).to_f32(),
                qs,
            });
        }
    }

    let input = (0..batch * n_in)
        .map(|index| (index as i32 % 19 - 9) as f32 * 0.07)
        .collect::<Vec<_>>();
    let mut expected = vec![0.0; batch * rows.len()];
    for item in 0..batch {
        for (local_row, row) in rows.clone().enumerate() {
            let mut sum = 0.0;
            for block in 0..blocks_per_row {
                let weight = &weight_blocks[row as usize * blocks_per_row + block];
                for lane in 0..32 {
                    sum += weight.scale
                        * weight.qs[lane] as f32
                        * input[item * n_in + block * 32 + lane];
                }
            }
            expected[item * rows.len() + local_row] = sum;
        }
    }

    Q8Fixture {
        batch,
        n_in,
        n_out,
        rows,
        weight_blocks,
        weight_bytes,
        input,
        expected,
    }
}

pub fn assert_close(actual: &[f32], expected: &[f32], atol: f32, rtol: f32) {
    assert_eq!(actual.len(), expected.len());
    for (index, (&actual, &expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() <= atol + rtol * expected.abs(),
            "mismatch at {index}: actual={actual}, expected={expected}"
        );
    }
}

#[cfg(feature = "vulkan")]
mod vulkan {
    use super::Q8Fixture;
    use rust_model_inference::{
        BackendError, BackendKind, ComponentId, DeviceDiscovery, DevicePlan, DeviceProvider,
        GGMLType, MemoryPlan, MetaValue, ProgramId, ProgramKind, ProgramPlan, ResidentTensorPlan,
        RunParams, SlotId, SlotKind, SlotPlan, SlotStorage, SourceFormat, SourceTensorRecord,
        TensorCatalog, TensorId, TensorInfo, TensorSource,
    };
    use std::sync::Arc;

    struct FixtureSource {
        info: TensorInfo,
        bytes: Vec<u8>,
    }

    impl TensorSource for FixtureSource {
        fn metadata(&self, _key: &str) -> Option<&MetaValue> {
            None
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            (name == self.info.name).then_some(&self.info)
        }

        fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
            (name == self.info.name).then_some(self.bytes.as_slice())
        }

        fn source_format(&self) -> SourceFormat {
            SourceFormat::Gguf
        }

        fn tensor_records(&self) -> Vec<SourceTensorRecord> {
            vec![SourceTensorRecord {
                info: self.info.clone(),
                segment_id: 0,
                segment_byte_range: 0..self.bytes.len() as u64,
                layer: None,
            }]
        }
    }

    pub fn require_backend(name: &str) {
        assert_eq!(name, "vulkan");
        let required = std::env::var("RMI_REQUIRE_BACKEND").ok();
        assert!(required.as_deref().is_none_or(|value| value == name));
        let provider =
            rust_model_inference::compute::VulkanProvider::new().unwrap_or_else(|error| {
                panic!("required Vulkan backend initialization failed: {error}")
            });
        let adapters = provider
            .enumerate()
            .unwrap_or_else(|error| panic!("required Vulkan discovery failed: {error}"));
        assert!(
            !adapters.is_empty(),
            "required Vulkan adapter is unavailable"
        );
    }

    pub fn run_q8_backend(
        backend: BackendKind,
        fixture: &Q8Fixture,
    ) -> Result<Vec<f32>, BackendError> {
        if backend != BackendKind::Vulkan {
            return Err(BackendError::BackendUnavailable { backend });
        }
        let provider = rust_model_inference::compute::VulkanProvider::new()?;
        let descriptor = provider
            .enumerate()?
            .into_iter()
            .next()
            .ok_or(BackendError::BackendUnavailable { backend })?;
        let catalog = Arc::new(
            TensorCatalog::from_sources(vec![(
                ComponentId::Llm,
                Arc::new(FixtureSource {
                    info: TensorInfo {
                        name: "weight".into(),
                        dims: vec![fixture.n_in as u64, fixture.n_out as u64],
                        ggml_type: GGMLType::Q8_0,
                        offset: 0,
                    },
                    bytes: fixture.weight_bytes.clone(),
                }),
            )])
            .map_err(|_| BackendError::InvalidHandle)?,
        );
        let row_bytes = (fixture.n_in / 32 * 34) as u64;
        let resident_bytes = fixture.rows.len() as u64 * row_bytes;
        let activation_bytes = (fixture.batch * fixture.n_in * size_of::<f32>()) as u64;
        let result_bytes = (fixture.batch * fixture.rows.len() * size_of::<f32>()) as u64;
        let plan = DevicePlan {
            descriptor: descriptor.clone(),
            tensors: vec![ResidentTensorPlan {
                tensor: TensorId(0),
                rows: fixture.rows.clone(),
                source_bytes: fixture.rows.start as u64 * row_bytes
                    ..fixture.rows.end as u64 * row_bytes,
                arena_offset: 0,
            }],
            slots: vec![
                SlotPlan {
                    id: SlotId(0),
                    kind: SlotKind::Activation,
                    storage: SlotStorage::F32,
                    byte_len: activation_bytes,
                    alignment: descriptor.buffer_alignment,
                    arena_offset: 0,
                },
                SlotPlan {
                    id: SlotId(1),
                    kind: SlotKind::Result,
                    storage: SlotStorage::F32,
                    byte_len: result_bytes,
                    alignment: descriptor.buffer_alignment,
                    arena_offset: activation_bytes,
                },
            ],
            programs: vec![ProgramPlan {
                id: ProgramId(0),
                kind: ProgramKind::Q8Rows {
                    tensor: TensorId(0),
                    rows: fixture.rows.clone(),
                    batch_capacity: fixture.batch as u32,
                },
                input: SlotId(0),
                output: SlotId(1),
                layer_ops: Vec::new(),
            }],
            memory: MemoryPlan {
                resident_bytes,
                scratch_bytes: activation_bytes + result_bytes,
                staging_bytes: resident_bytes.max(activation_bytes).max(result_bytes),
                required_bytes: resident_bytes * 2 + activation_bytes + result_bytes,
                largest_allocation_bytes: resident_bytes.max(activation_bytes + result_bytes),
                ..MemoryPlan::default()
            },
        };
        let mut session = provider.open(&descriptor, &plan, Arc::clone(&catalog))?;
        session.write_f32(SlotId(0), &fixture.input)?;
        let fence = session.submit(
            ProgramId(0),
            &RunParams {
                token_count: fixture.batch as u32,
                position_start: 0,
                mrope_positions: &[],
                token_ids: &[],
            },
        )?;
        session.wait(fence)?;
        let mut actual = vec![0.0; fixture.expected.len()];
        session.read_f32(SlotId(1), &mut actual)?;
        Ok(actual)
    }
}

#[cfg(feature = "vulkan")]
pub use vulkan::{require_backend, run_q8_backend};
