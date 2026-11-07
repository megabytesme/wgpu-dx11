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
    pub requires_bias_compatible: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TextureInfo {
    pub size: [u32; 3],
    pub mip_levels: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SamplerLod {
    Unchanged,
    SingleMip,
    RaisedMaximum { original: f32, anisotropic: bool },
}

impl SamplerLod {
    pub(super) fn compatible(self, mip_levels: u32) -> bool {
        if mip_levels == 0 {
            return false;
        }
        match self {
            Self::Unchanged => true,
            Self::SingleMip => mip_levels == 1,
            Self::RaisedMaximum {
                original,
                anisotropic,
            } => mip_levels == 1 || (!anisotropic && original >= (mip_levels - 1) as f32),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct SamplingResources {
    pub textures: Vec<TextureInfo>,
    pub samplers_wrap: Vec<bool>,
    pub samplers_lod: Vec<SamplerLod>,
    pub samplers_bias_compatible: Vec<bool>,
}

impl SamplingResources {
    pub(super) fn append(&mut self, other: &Self) {
        self.textures.extend_from_slice(&other.textures);
        self.samplers_wrap.extend_from_slice(&other.samplers_wrap);
        self.samplers_lod.extend_from_slice(&other.samplers_lod);
        self.samplers_bias_compatible
            .extend_from_slice(&other.samplers_bias_compatible);
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
        let texture_kind = shader_binding_kind(module, texture);
        let sampler_kind = shader_binding_kind(module, sampler);
        if stage != naga::ShaderStage::Fragment
            || !matches!(
                texture_kind,
                Some(ShaderBindingKind::TextureFloat | ShaderBindingKind::TextureDepth)
            )
            || !matches!(
                sampler_kind,
                Some(ShaderBindingKind::Sampler | ShaderBindingKind::SamplerComparison)
            )
            || (texture_kind == Some(ShaderBindingKind::TextureDepth))
                != (sampler_kind == Some(ShaderBindingKind::SamplerComparison))
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
            texture_slot: find(texture, texture_kind.ok_or(())?)?,
            sampler_slot: find(sampler, sampler_kind.ok_or(())?)?,
            requires_bias_compatible: false,
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
        let (Some(texture), Some(&wraps), Some(&lod), Some(&bias_compatible)) = (
            resources.textures.get(pair.texture_slot),
            resources.samplers_wrap.get(pair.sampler_slot),
            resources.samplers_lod.get(pair.sampler_slot),
            resources.samplers_bias_compatible.get(pair.sampler_slot),
        ) else {
            return false;
        };
        texture.size.iter().all(|dimension| *dimension != 0)
            && (!pair.requires_bias_compatible || bias_compatible)
            && texture.mip_levels
                <= texture.size[0]
                    .max(texture.size[1])
                    .max(texture.size[2])
                    .ilog2()
                    + 1
            && lod.compatible(texture.mip_levels)
            && (texture.mip_levels == 1
                || texture
                    .size
                    .iter()
                    .all(|dimension| dimension.is_power_of_two()))
            && (level.as_raw() >= FeatureLevel::L10_0.as_raw()
                || matches!(full_npot, Query::Value(true))
                || !wraps
                || texture
                    .size
                    .iter()
                    .all(|dimension| dimension.is_power_of_two()))
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
            (7, 0, ShaderBindingKind::TextureFloat),
            (42, 1, ShaderBindingKind::TextureFloat),
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

    #[test]
    fn three_dimensional_texture_relationship_keeps_all_axes_in_compatibility_check() {
        let source = r#"
            @group(0) @binding(4) var volume: texture_3d<f32>;
            @group(0) @binding(9) var wrapping: sampler;
            @fragment fn sample_volume() -> @location(0) vec4f {
                return textureSample(volume, wrapping, vec3f(0.5));
            }
        "#;
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 4,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 9,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        let pairs = relationships(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "sample_volume",
            &bindings,
        )
        .unwrap();
        let mut resources = SamplingResources {
            textures: alloc::vec![TextureInfo {
                size: [4, 4, 3],
                mip_levels: 1,
            }],
            samplers_wrap: alloc::vec![true],
            samplers_lod: alloc::vec![SamplerLod::Unchanged],
            samplers_bias_compatible: alloc::vec![true],
        };
        assert!(!compatible(
            &pairs,
            &resources,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        resources.textures[0].size[2] = 4;
        assert!(compatible(
            &pairs,
            &resources,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
    }

    fn resources() -> SamplingResources {
        SamplingResources {
            textures: alloc::vec![
                TextureInfo {
                    size: [4, 8, 1],
                    mip_levels: 1
                },
                TextureInfo {
                    size: [3, 5, 1],
                    mip_levels: 1
                }
            ],
            samplers_wrap: alloc::vec![true, false],
            samplers_lod: alloc::vec![SamplerLod::Unchanged; 2],
            samplers_bias_compatible: alloc::vec![true; 2],
        }
    }

    #[test]
    fn biased_pairs_check_current_sampler_and_ignore_unused_samplers() {
        let (module, info, bindings) = fixture();
        let mut pairs = relationships(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "valid",
            &bindings,
        )
        .unwrap();
        let mut actual = resources();
        actual.samplers_bias_compatible[0] = false;
        assert!(compatible(
            &pairs,
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        for pair in &mut pairs {
            pair.requires_bias_compatible = true;
        }
        assert!(!compatible(
            &pairs,
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        actual.samplers_bias_compatible[0] = true;
        assert!(compatible(
            &pairs,
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        actual.samplers_bias_compatible.push(false);
        assert!(compatible(
            &pairs,
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        actual.samplers_bias_compatible.clear();
        assert!(!compatible(
            &pairs,
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
    }

    #[test]
    fn entry_point_sampling_set_recognizes_comparison_texture_pairs() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var shadow: texture_depth_2d;
            @group(0) @binding(42) var comparison: sampler_comparison;
            fn compare_depth(t: texture_depth_2d, s: sampler_comparison, uv: vec2f) -> f32 {
                return textureSampleCompare(t, s, uv, 0.5);
            }
            @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {
                let value = compare_depth(shadow, comparison, uv);
                return vec4f(value, value, value, 1.0);
            }
            "#,
        )
        .expect("parse depth comparison sampling shader");
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .expect("validate depth comparison sampling shader");
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 7,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureDepth,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 42,
                },
                slot: 0,
                kind: ShaderBindingKind::SamplerComparison,
            },
        ];
        let pairs = relationships(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            &bindings,
        )
        .expect("resolve comparison resource identities");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].texture.binding, 7);
        assert_eq!(pairs[0].sampler.binding, 42);

        let mut wrong_bindings = bindings;
        wrong_bindings[0].kind = ShaderBindingKind::TextureFloat;
        assert!(relationships(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            &wrong_bindings,
        )
        .is_err());
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
            (0, 0, ShaderBindingKind::TextureFloat),
            (1, 1, ShaderBindingKind::TextureFloat),
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
                    size: [4, 8, 1],
                    mip_levels: 1
                }],
                samplers_wrap: alloc::vec![],
                samplers_lod: alloc::vec![],
                samplers_bias_compatible: alloc::vec![],
            },
            SamplingResources {
                textures: alloc::vec![TextureInfo {
                    size: [3, 5, 1],
                    mip_levels: 1
                }],
                samplers_wrap: alloc::vec![],
                samplers_lod: alloc::vec![],
                samplers_bias_compatible: alloc::vec![],
            },
            SamplingResources {
                textures: alloc::vec![],
                samplers_wrap: alloc::vec![true],
                samplers_lod: alloc::vec![SamplerLod::Unchanged],
                samplers_bias_compatible: alloc::vec![true],
            },
            SamplingResources {
                textures: alloc::vec![],
                samplers_wrap: alloc::vec![false],
                samplers_lod: alloc::vec![SamplerLod::Unchanged],
                samplers_bias_compatible: alloc::vec![true],
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
        changed.textures[0].size = [4, 3, 1];
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
    fn selected_cross_group_pairs_enforce_declared_filterability() {
        use super::super::hal_bind_group::{BindingPlan, GroupKind};

        let source = r#"
            @group(0) @binding(7) var filterable: texture_2d<f32>;
            @group(1) @binding(7) var unfilterable: texture_2d<f32>;
            @group(0) @binding(100) var filtering: sampler;
            @group(1) @binding(100) var nonfiltering: sampler;
            fn sample_pair(t: texture_2d<f32>, s: sampler) -> vec4f {
                return textureSample(t, s, vec2f(0.5));
            }
            @fragment fn valid() -> @location(0) vec4f {
                return sample_pair(filterable, filtering) + sample_pair(unfilterable, nonfiltering);
            }
            @fragment fn invalid() -> @location(0) vec4f {
                return sample_pair(unfilterable, filtering);
            }
            @fragment fn uses_shared() -> @location(0) vec4f {
                return sample_pair(filterable, nonfiltering) + sample_pair(unfilterable, nonfiltering);
            }
            @fragment fn multiple_valid() -> @location(0) vec4f {
                return sample_pair(filterable, filtering) + sample_pair(filterable, nonfiltering);
            }
            @fragment fn multiple_invalid() -> @location(0) vec4f {
                return sample_pair(unfilterable, filtering) + sample_pair(unfilterable, nonfiltering);
            }
        "#;
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let mut first = GroupKind::EMPTY;
        first.textures[0] = Some(7);
        first.samplers[0] = Some(100);
        let mut second = first;
        second.texture_unfilterable_mask = 1;
        second.sampler_non_filtering_mask = 1;
        let plan = BindingPlan::new(&[Some(first), Some(second)]).unwrap();
        let bindings = plan.stage_bindings(wgt::ShaderStages::FRAGMENT);
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
        assert_eq!(valid.len(), 2);
        assert!(plan.sampling_pairs_compatible(&valid));
        assert!(!plan.sampling_pairs_compatible(&get("invalid")));
        assert!(plan.sampling_pairs_compatible(&get("uses_shared")));
        assert!(plan.sampling_pairs_compatible(&get("multiple_valid")));
        assert!(!plan.sampling_pairs_compatible(&get("multiple_invalid")));

        let mut wrong_slot = valid.clone();
        wrong_slot[0].texture_slot += 1;
        assert!(!plan.sampling_pairs_compatible(&wrong_slot));
        let mut missing = valid;
        missing[0].sampler.binding = 999;
        assert!(!plan.sampling_pairs_compatible(&missing));
    }

    #[test]
    fn mip_lod_validation_uses_only_selected_sampling_pairs() {
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
        let mut actual = resources();
        actual.textures[0].mip_levels = 4;
        actual.samplers_lod[0] = SamplerLod::RaisedMaximum {
            original: 3.0,
            anisotropic: false,
        };
        actual.samplers_lod[1] = SamplerLod::SingleMip;
        assert!(compatible(
            &get("valid"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        assert!(!compatible(
            &get("uses_shared"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
        assert!(compatible(
            &get("invalid"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
        actual.samplers_lod[0] = SamplerLod::RaisedMaximum {
            original: 2.0,
            anisotropic: false,
        };
        assert!(!compatible(
            &get("valid"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        actual.samplers_lod[0] = SamplerLod::RaisedMaximum {
            original: 32.0,
            anisotropic: true,
        };
        assert!(!compatible(
            &get("valid"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        actual.samplers_lod[0] = SamplerLod::Unchanged;
        assert!(compatible(
            &get("valid"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(false)
        ));
        actual.textures[0].mip_levels = 5;
        assert!(!compatible(
            &get("valid"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
        actual.textures[0].mip_levels = 0;
        assert!(!compatible(
            &get("valid"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
        actual.textures[0].mip_levels = 2;
        actual.textures[0].size = [3, 8, 1];
        assert!(!compatible(
            &get("valid"),
            &actual,
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
        actual.textures[0].size = [4, 8, 1];
        actual.samplers_lod.clear();
        assert!(!compatible(
            &get("valid"),
            &actual,
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
            requires_bias_compatible: false,
        };
        assert!(!compatible(
            &[pair],
            &resources(),
            FeatureLevel::L9_1,
            Query::Value(true)
        ));
    }
}
