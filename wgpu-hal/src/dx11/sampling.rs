//! Entry-point sampling relationships and FL9 texture/sampler compatibility.
use super::{
    capabilities::Query,
    device::FeatureLevel,
    shader::{shader_binding_kind, ShaderBinding, ShaderBindingKind},
};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SamplingPair {
    pub texture: naga::ResourceBinding,
    pub sampler: naga::ResourceBinding,
    pub texture_slot: usize,
    pub sampler_slot: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TextureInfo {
    pub size: [u32; 2],
    pub mip_levels: u32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SamplingResources {
    pub textures: Vec<TextureInfo>,
    pub samplers_wrap: Vec<bool>,
}

impl SamplingResources {
    pub(super) fn append(&mut self, other: &Self) {
        self.textures.extend_from_slice(&other.textures);
        self.samplers_wrap.extend_from_slice(&other.samplers_wrap);
    }
}

pub(super) fn relationships(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    bindings: &[ShaderBinding],
) -> Result<Vec<SamplingPair>, ()> {
    let index = module
        .entry_points
        .iter()
        .position(|ep| ep.stage == stage && ep.name == entry)
        .ok_or(())?;
    let mut pairs = Vec::new();
    for pair in &info.get_entry_point(index).sampling_set {
        let texture = module
            .global_variables
            .try_get(pair.image)
            .map_err(|_| ())?;
        let sampler = module
            .global_variables
            .try_get(pair.sampler)
            .map_err(|_| ())?;
        if stage != naga::ShaderStage::Fragment
            || shader_binding_kind(module, texture) != Some(ShaderBindingKind::Texture2dFloat)
            || shader_binding_kind(module, sampler) != Some(ShaderBindingKind::Sampler)
        {
            return Err(());
        }
        let texture = texture.binding.ok_or(())?;
        let sampler = sampler.binding.ok_or(())?;
        let find = |binding, kind| {
            let mut matches = bindings
                .iter()
                .filter(|mapping| mapping.binding == binding && mapping.kind == kind);
            let mapping = matches.next().ok_or(())?;
            if matches.next().is_some() {
                return Err(());
            }
            Ok(mapping.slot as usize)
        };
        pairs.push(SamplingPair {
            texture,
            sampler,
            texture_slot: find(texture, ShaderBindingKind::Texture2dFloat)?,
            sampler_slot: find(sampler, ShaderBindingKind::Sampler)?,
        });
    }
    pairs.sort_unstable_by_key(|pair| {
        (
            pair.texture.group,
            pair.texture.binding,
            pair.sampler.group,
            pair.sampler.binding,
        )
    });
    Ok(pairs)
}

pub(super) fn compatible(
    pairs: &[SamplingPair],
    resources: &SamplingResources,
    level: FeatureLevel,
    full_npot: Query<bool>,
) -> bool {
    pairs.iter().all(|pair| {
        let (Some(texture), Some(&wraps)) = (
            resources.textures.get(pair.texture_slot),
            resources.samplers_wrap.get(pair.sampler_slot),
        ) else {
            return false;
        };
        texture.mip_levels == 1
            && texture.size[0] != 0
            && texture.size[1] != 0
            && (level.as_raw() >= FeatureLevel::L10_0.as_raw()
                || matches!(full_npot, Query::Value(true))
                || !wraps
                || (texture.size[0].is_power_of_two() && texture.size[1].is_power_of_two()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    fn fixture() -> (naga::Module, naga::valid::ModuleInfo, Vec<ShaderBinding>) {
        let source = r#"
            @group(0) @binding(7) var a: texture_2d<f32>;
            @group(0) @binding(42) var b: texture_2d<f32>;
            @group(0) @binding(100) var wrap_s: sampler;
            @group(0) @binding(200) var clamp_s: sampler;
            fn sample_pair(t: texture_2d<f32>, s: sampler) -> vec4f {
                return textureSample(t, s, vec2f(0.5));
            }
            @fragment fn valid() -> @location(0) vec4f {
                return sample_pair(a, wrap_s) + sample_pair(b, clamp_s);
            }
            @fragment fn invalid() -> @location(0) vec4f {
                return sample_pair(b, wrap_s);
            }
            @fragment fn uses_shared() -> @location(0) vec4f {
                return sample_pair(a, clamp_s) + sample_pair(b, clamp_s);
            }
            @fragment fn multiple() -> @location(0) vec4f {
                return sample_pair(a, wrap_s) + sample_pair(a, clamp_s);
            }
        "#;
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let bindings = [
            (7, 0, ShaderBindingKind::Texture2dFloat),
            (42, 1, ShaderBindingKind::Texture2dFloat),
            (100, 0, ShaderBindingKind::Sampler),
            (200, 1, ShaderBindingKind::Sampler),
        ]
        .into_iter()
        .map(|(binding, slot, kind)| ShaderBinding {
            binding: naga::ResourceBinding { group: 0, binding },
            slot,
            kind,
        })
        .collect();
        (module, info, bindings)
    }

    fn resources() -> SamplingResources {
        SamplingResources {
            textures: alloc::vec![
                TextureInfo {
                    size: [4, 8],
                    mip_levels: 1
                },
                TextureInfo {
                    size: [3, 5],
                    mip_levels: 1
                }
            ],
            samplers_wrap: alloc::vec![true, false],
        }
    }

    #[test]
    fn cross_group_relationships_use_global_slots_and_actual_pairs() {
        let module = naga::front::wgsl::parse_str(r#"
            @group(0) @binding(7) var pot: texture_2d<f32>;
            @group(1) @binding(7) var npot: texture_2d<f32>;
            @group(2) @binding(7) var wrap: sampler;
            @group(3) @binding(7) var clamp: sampler;
            fn sample_pair(t: texture_2d<f32>, s: sampler) -> vec4f { return textureSample(t, s, vec2f(0.5)); }
            @fragment fn valid() -> @location(0) vec4f { return sample_pair(pot, wrap) + sample_pair(npot, clamp); }
            @fragment fn invalid() -> @location(0) vec4f { return sample_pair(npot, wrap); }
        "#).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let bindings: Vec<_> = [
            (0, 0, ShaderBindingKind::Texture2dFloat),
            (1, 1, ShaderBindingKind::Texture2dFloat),
            (2, 0, ShaderBindingKind::Sampler),
            (3, 1, ShaderBindingKind::Sampler),
        ]
        .into_iter()
        .map(|(group, slot, kind)| ShaderBinding {
            binding: naga::ResourceBinding { group, binding: 7 },
            slot,
            kind,
        })
        .collect();
        let get = |entry| {
            relationships(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                entry,
                &bindings,
            )
            .unwrap()
        };
        let valid = get("valid");
        assert_eq!(
            valid
                .iter()
                .map(|p| (
                    p.texture.group,
                    p.sampler.group,
                    p.texture_slot,
                    p.sampler_slot
                ))
                .collect::<Vec<_>>(),
            [(0, 2, 0, 0), (1, 3, 1, 1)]
        );
        let mut combined = SamplingResources::default();
        for group in [
            SamplingResources {
                textures: alloc::vec![TextureInfo {
                    size: [4, 8],
                    mip_levels: 1
                }],
                samplers_wrap: alloc::vec![],
            },
            SamplingResources {
                textures: alloc::vec![TextureInfo {
                    size: [3, 5],
                    mip_levels: 1
                }],
                samplers_wrap: alloc::vec![],
            },
            SamplingResources {
                textures: alloc::vec![],
                samplers_wrap: alloc::vec![true],
            },
            SamplingResources {
                textures: alloc::vec![],
                samplers_wrap: alloc::vec![false],
            },
        ] {
            combined.append(&group);
        }
        assert!(compatible(
            &valid,
            &combined,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        let invalid = get("invalid");
        assert!(!compatible(
            &invalid,
            &combined,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        assert!(!compatible(
            &invalid,
            &combined,
            FeatureLevel::L9_1,
            Query::Unavailable(-1)
        ));
        assert!(compatible(
            &invalid,
            &combined,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
        combined.samplers_wrap.pop();
        assert!(!compatible(
            &valid,
            &combined,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
    }

    #[test]
    fn sampling_relationships_follow_helpers_and_selected_entry_point() {
        let (module, info, bindings) = fixture();
        let get = |entry| {
            relationships(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                entry,
                &bindings,
            )
            .unwrap()
        };
        let valid = get("valid");
        assert_eq!(
            valid
                .iter()
                .map(|p| (p.texture.binding, p.sampler.binding))
                .collect::<Vec<_>>(),
            [(7, 100), (42, 200)]
        );
        assert!(compatible(
            &valid,
            &resources(),
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        let invalid = get("invalid");
        assert_eq!(
            (invalid[0].texture.binding, invalid[0].sampler.binding),
            (42, 100)
        );
        assert!(!compatible(
            &invalid,
            &resources(),
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        assert!(compatible(
            &invalid,
            &resources(),
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
        assert!(!compatible(
            &invalid,
            &resources(),
            FeatureLevel::L9_1,
            Query::Unavailable(-1)
        ));
        let shared = get("uses_shared");
        assert_eq!(
            shared
                .iter()
                .map(|p| (p.texture_slot, p.sampler_slot))
                .collect::<Vec<_>>(),
            [(0, 1), (1, 1)]
        );
        assert!(compatible(
            &shared,
            &resources(),
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        let multiple = get("multiple");
        assert_eq!(
            multiple
                .iter()
                .map(|p| (p.texture_slot, p.sampler_slot))
                .collect::<Vec<_>>(),
            [(0, 0), (0, 1)]
        );
        assert!(compatible(
            &multiple,
            &resources(),
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        let mut changed = resources();
        changed.textures[0].size = [4, 3];
        assert!(!compatible(
            &multiple,
            &changed,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        assert!(compatible(
            &multiple,
            &changed,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
        assert!(compatible(
            &multiple,
            &changed,
            FeatureLevel::L10_0,
            Query::Unavailable(-1)
        ));
        changed.textures[0].mip_levels = 2;
        assert!(!compatible(
            &multiple,
            &changed,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
    }

    #[test]
    fn sampling_relationships_fail_closed_on_missing_or_invalid_bindings() {
        let (module, info, mut bindings) = fixture();
        assert!(relationships(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "missing",
            &bindings
        )
        .is_err());
        assert!(relationships(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "valid",
            &bindings
        )
        .is_err());
        let duplicate = bindings[0];
        bindings.push(duplicate);
        assert!(relationships(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "valid",
            &bindings
        )
        .is_err());
        bindings.pop();
        bindings.pop();
        assert!(relationships(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "valid",
            &bindings
        )
        .is_err());
        let (mut unbound_module, unbound_info, unbound_bindings) = fixture();
        let handle = unbound_module
            .global_variables
            .iter()
            .find(|(_, global)| {
                global.binding
                    == Some(naga::ResourceBinding {
                        group: 0,
                        binding: 7,
                    })
            })
            .unwrap()
            .0;
        unbound_module.global_variables[handle].binding = None;
        assert!(relationships(
            &unbound_module,
            &unbound_info,
            naga::ShaderStage::Fragment,
            "valid",
            &unbound_bindings
        )
        .is_err());
        let pair = SamplingPair {
            texture: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            sampler: naga::ResourceBinding {
                group: 0,
                binding: 100,
            },
            texture_slot: 100,
            sampler_slot: 0,
        };
        assert!(!compatible(
            &[pair],
            &resources(),
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
    }
}
