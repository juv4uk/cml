//! Backend-neutral accelerator discovery and selection policy.
//!
//! The planner ranks only descriptors supplied by live backend discovery. It
//! never turns a planned CUDA/ROCm/Level Zero implementation into an available
//! device claim.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorApi {
    /// Portable WebGPU-family execution, currently implemented by `wgpu`.
    Wgpu,
    Cuda,
    Rocm,
    OneApiLevelZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorClass {
    IntegratedGpu,
    DiscreteGpu,
    VirtualGpu,
    Cpu,
    Unknown,
}

/// Discovery result before a backend is allowed to enter accelerator selection.
///
/// `Planned` and `Unsupported` are durable negative evidence, not devices. Only
/// `Live` capabilities may be converted into an [`AcceleratorDescriptor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityStatus {
    Live,
    Planned,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCapability {
    pub name: String,
    pub vendor: AcceleratorVendor,
    pub api: AcceleratorApi,
    pub class: AcceleratorClass,
    pub status: CapabilityStatus,
    /// Human-readable probe evidence or the reason the path is unavailable.
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorDescriptor {
    pub name: String,
    pub vendor: AcceleratorVendor,
    pub api: AcceleratorApi,
    pub class: AcceleratorClass,
}

impl BackendCapability {
    /// Admit a backend into selection only after live discovery produced it.
    pub fn into_live_descriptor(self) -> Option<AcceleratorDescriptor> {
        (self.status == CapabilityStatus::Live).then_some(AcceleratorDescriptor {
            name: self.name,
            vendor: self.vendor,
            api: self.api,
            class: self.class,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionPolicy {
    /// Prefer the cross-vendor implementation when both paths are live.
    PortableFirst,
    /// Prefer a matching vendor API, retaining wgpu as the fallback.
    VendorOptimized,
}

pub fn select_accelerator<'a>(
    candidates: &'a [AcceleratorDescriptor],
    policy: SelectionPolicy,
) -> Option<&'a AcceleratorDescriptor> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            rank(candidate, policy).map(|rank| (rank, index, candidate))
        })
        .min_by_key(|(rank, index, _)| (*rank, *index))
        .map(|(_, _, candidate)| candidate)
}

fn rank(candidate: &AcceleratorDescriptor, policy: SelectionPolicy) -> Option<u8> {
    if !matches!(
        candidate.class,
        AcceleratorClass::IntegratedGpu
            | AcceleratorClass::DiscreteGpu
            | AcceleratorClass::VirtualGpu
    ) {
        return None;
    }

    let portable = candidate.api == AcceleratorApi::Wgpu;
    let vendor_match = matches!(
        (candidate.vendor, candidate.api),
        (AcceleratorVendor::Nvidia, AcceleratorApi::Cuda)
            | (AcceleratorVendor::Amd, AcceleratorApi::Rocm)
            | (AcceleratorVendor::Intel, AcceleratorApi::OneApiLevelZero)
    );
    if !portable && !vendor_match {
        return None;
    }

    Some(match (policy, portable) {
        (SelectionPolicy::PortableFirst, true) => 0,
        (SelectionPolicy::PortableFirst, false) => 1,
        (SelectionPolicy::VendorOptimized, false) => 0,
        (SelectionPolicy::VendorOptimized, true) => 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(name: &str, vendor: AcceleratorVendor, api: AcceleratorApi) -> AcceleratorDescriptor {
        AcceleratorDescriptor {
            name: name.into(),
            vendor,
            api,
            class: AcceleratorClass::DiscreteGpu,
        }
    }

    #[test]
    fn portable_policy_is_vendor_neutral() {
        for vendor in [
            AcceleratorVendor::Nvidia,
            AcceleratorVendor::Amd,
            AcceleratorVendor::Intel,
        ] {
            let candidates = vec![
                gpu("portable", vendor, AcceleratorApi::Wgpu),
                gpu(
                    "native",
                    vendor,
                    match vendor {
                        AcceleratorVendor::Nvidia => AcceleratorApi::Cuda,
                        AcceleratorVendor::Amd => AcceleratorApi::Rocm,
                        AcceleratorVendor::Intel => AcceleratorApi::OneApiLevelZero,
                        _ => unreachable!(),
                    },
                ),
            ];
            assert_eq!(
                select_accelerator(&candidates, SelectionPolicy::PortableFirst)
                    .unwrap()
                    .name,
                "portable"
            );
        }
    }

    #[test]
    fn optimized_policy_uses_matching_vendor_api() {
        let candidates = vec![
            gpu("amd-wgpu", AcceleratorVendor::Amd, AcceleratorApi::Wgpu),
            gpu("amd-rocm", AcceleratorVendor::Amd, AcceleratorApi::Rocm),
        ];
        assert_eq!(
            select_accelerator(&candidates, SelectionPolicy::VendorOptimized)
                .unwrap()
                .name,
            "amd-rocm"
        );
    }

    #[test]
    fn cpu_and_mismatched_vendor_apis_are_rejected() {
        let candidates = vec![
            AcceleratorDescriptor {
                name: "llvmpipe".into(),
                vendor: AcceleratorVendor::Other,
                api: AcceleratorApi::Wgpu,
                class: AcceleratorClass::Cpu,
            },
            gpu("not-cuda", AcceleratorVendor::Amd, AcceleratorApi::Cuda),
        ];
        assert_eq!(
            select_accelerator(&candidates, SelectionPolicy::PortableFirst),
            None
        );
    }

    #[test]
    fn unavailable_capabilities_never_become_planner_candidates() {
        for status in [CapabilityStatus::Planned, CapabilityStatus::Unsupported] {
            let capability = BackendCapability {
                name: "Intel HD Graphics 530".into(),
                vendor: AcceleratorVendor::Intel,
                api: AcceleratorApi::OneApiLevelZero,
                class: AcceleratorClass::IntegratedGpu,
                status,
                evidence: "backend probe did not produce a live device".into(),
            };
            assert_eq!(capability.into_live_descriptor(), None);
        }
    }

    #[test]
    fn live_capability_can_enter_the_existing_selection_policy() {
        let descriptor = BackendCapability {
            name: "live Intel GPU".into(),
            vendor: AcceleratorVendor::Intel,
            api: AcceleratorApi::OneApiLevelZero,
            class: AcceleratorClass::IntegratedGpu,
            status: CapabilityStatus::Live,
            evidence: "Level Zero enumeration returned device zero".into(),
        }
        .into_live_descriptor()
        .unwrap();

        assert_eq!(
            select_accelerator(&[descriptor], SelectionPolicy::VendorOptimized)
                .unwrap()
                .name,
            "live Intel GPU"
        );
    }
}
