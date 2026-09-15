//! Intel Core i5-6400 (Skylake) CPU capability profile parser and capability authority (#59).
//!
//! # Architecture and Philosophy
//!
//! - **Target Hardware Profile Authority**: Consumes `my-lisp/lib/machine/cpu/intel-core-i5-6400.lisp`.
//!   The compiler does not hardcode "Skylake implies everything"; capabilities are determined
//!   by reading the exact target profile.
//! - **Fail-Closed Capability Enforcement**: Features recorded as unavailable (e.g. AVX-512, AMX, TSX)
//!   must never be emitted. Unknown capability state fails closed to the admitted scalar path.
//! - **Runtime Gate Verification**: Gated extensions (such as AVX and AVX2) require checking
//!   host CPUID flags and OSXSAVE/XGETBV state before vector paths are selected.
//! - **Capability Provenance**: Every selection decision records why a feature was selected
//!   or rejected, making target optimization policy auditable.
//!
//! # Українська документація (Ukrainian Documentation)
//!
//! Цей модуль зчитує та валідує профіль апаратних можливостей процесора Intel Core i5-6400 (Skylake)
//! із файлу `lib/machine/cpu/intel-core-i5-6400.lisp`. Він реалізує fail-closed перевірку розширень,
//! валідацію CPUID/XGETBV для AVX2 та веде журнал походження можливостей (capability provenance).

use std::collections::{HashMap, HashSet};

use crate::ast::Expr;
use crate::parser::parse;

/// The i5-6400 CPU profile, compiled in directly from the `external/my-lisp`
/// submodule (SUBMODULE-DEPENDENCY-MODEL-2026-09-16) — never a hand-copied
/// duplicate. `include_str!` also means a missing/uninitialized submodule
/// fails the *build*, not silently at runtime with stale data: this repo's
/// own architecture explicitly forbids a silent fallback for CPU profiles.
pub const CANONICAL_I5_6400_PROFILE_LISP: &str =
    include_str!("../external/my-lisp/lib/machine/cpu/intel-core-i5-6400.lisp");

/// Structured model of a CPU capability profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuProfile {
    pub cpu: String,
    pub microarchitecture: String,
    pub isa: String,
    pub mode: String,
    pub supported_extensions: HashSet<String>,
    pub gated_extensions: HashMap<String, String>,
    pub platform_gated_extensions: HashMap<String, String>,
    pub virtualization_capabilities: HashMap<String, String>,
    pub unavailable_extensions: HashSet<String>,
    pub execution_policies: HashMap<String, String>,
}

/// Execution vector mode requested by caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorMode {
    /// Automatically select vector path if target profile and runtime host permit.
    Auto,
    /// Force scalar fallback path.
    ForcedScalar,
    /// Force AVX2 vector path (fails closed if unavailable).
    ForcedAvx2,
}

/// Minimum buffer length threshold where AVX2 loop overhead becomes profitable over scalar.
pub const AVX2_CROSSOVER_THRESHOLD: usize = 8;

/// Audit provenance explaining why a vector instruction set was or was not selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityProvenance {
    pub extension: String,
    pub selected: bool,
    pub reason: String,
}

impl CpuProfile {
    /// Parses a `cpu-profile/1` S-expression string into a structured `CpuProfile`.
    pub fn parse(source: &str) -> Result<Self, String> {
        let exprs = parse(source).map_err(|e| format!("parse error in cpu profile: {e}"))?;
        let root = exprs
            .into_iter()
            .find(|e| match e {
                Expr::List(items) => items.first().is_some_and(|h| h.is_symbol("cpu-profile/1")),
                _ => false,
            })
            .ok_or_else(|| "missing (cpu-profile/1 ...) root form".to_string())?;

        let items = match root {
            Expr::List(list) => list,
            _ => unreachable!(),
        };

        let mut cpu = String::new();
        let mut microarchitecture = String::new();
        let mut isa = String::new();
        let mut mode = String::new();
        let mut supported_extensions = HashSet::new();
        let mut gated_extensions = HashMap::new();
        let mut platform_gated_extensions = HashMap::new();
        let mut virtualization_capabilities = HashMap::new();
        let mut unavailable_extensions = HashSet::new();
        let mut execution_policies = HashMap::new();

        for item in items.into_iter().skip(1) {
            let Expr::List(clause) = item else {
                continue;
            };
            if clause.is_empty() {
                continue;
            }
            let head = match &clause[0] {
                Expr::Symbol(s) => s.as_str(),
                _ => continue,
            };

            match head {
                "cpu" if clause.len() >= 2 => {
                    if let Expr::Symbol(name) = &clause[1] {
                        cpu = name.clone();
                    }
                }
                "microarchitecture" if clause.len() >= 2 => {
                    if let Expr::Symbol(name) = &clause[1] {
                        microarchitecture = name.clone();
                    }
                }
                "isa" if clause.len() >= 2 => {
                    if let Expr::Symbol(name) = &clause[1] {
                        isa = name.clone();
                    }
                }
                "mode" if clause.len() >= 2 => {
                    if let Expr::Symbol(name) = &clause[1] {
                        mode = name.clone();
                    }
                }
                "supported-extension" if clause.len() >= 2 => {
                    if let Expr::Symbol(name) = &clause[1] {
                        supported_extensions.insert(name.to_ascii_uppercase());
                    }
                }
                "gated-extension" if clause.len() >= 3 => {
                    if let Expr::Symbol(ext) = &clause[1] {
                        let gate_desc = match &clause[2] {
                            Expr::List(g) if g.len() >= 2 && g[0].is_symbol("gate") => {
                                match &g[1] {
                                    Expr::Symbol(s) => s.clone(),
                                    _ => "unknown".to_string(),
                                }
                            }
                            _ => "unknown".to_string(),
                        };
                        gated_extensions.insert(ext.to_ascii_uppercase(), gate_desc);
                    }
                }
                "platform-gated-extension" if clause.len() >= 3 => {
                    if let Expr::Symbol(ext) = &clause[1] {
                        let gate_desc = match &clause[2] {
                            Expr::List(g) if g.len() >= 2 && g[0].is_symbol("gate") => {
                                match &g[1] {
                                    Expr::Symbol(s) => s.clone(),
                                    _ => "unknown".to_string(),
                                }
                            }
                            _ => "unknown".to_string(),
                        };
                        platform_gated_extensions.insert(ext.to_ascii_uppercase(), gate_desc);
                    }
                }
                "virtualization-capability" if clause.len() >= 3 => {
                    if let (Expr::Symbol(feat), Expr::Symbol(status)) = (&clause[1], &clause[2]) {
                        virtualization_capabilities
                            .insert(feat.to_ascii_uppercase(), status.clone());
                    }
                }
                "unavailable-extension" if clause.len() >= 2 => {
                    if let Expr::Symbol(ext) = &clause[1] {
                        unavailable_extensions.insert(ext.to_ascii_uppercase());
                    }
                }
                "execution-policy" => {
                    for policy_item in clause.into_iter().skip(1) {
                        if let Expr::List(pair) = policy_item {
                            if pair.len() >= 2 {
                                if let (Expr::Symbol(k), Expr::Symbol(v)) = (&pair[0], &pair[1]) {
                                    execution_policies.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(Self {
            cpu,
            microarchitecture,
            isa,
            mode,
            supported_extensions,
            gated_extensions,
            platform_gated_extensions,
            virtualization_capabilities,
            unavailable_extensions,
            execution_policies,
        })
    }

    /// Loads the canonical i5-6400 profile compiled in from `external/my-lisp`
    /// (see `CANONICAL_I5_6400_PROFILE_LISP`) — one channel, no path guessing.
    pub fn load_skylake_i5_6400() -> Result<Self, String> {
        Self::parse(CANONICAL_I5_6400_PROFILE_LISP)
    }

    /// Returns whether an extension is explicitly marked unavailable (e.g. AVX-512, AMX, TSX).
    pub fn is_unavailable(&self, extension: &str) -> bool {
        self.unavailable_extensions
            .contains(&extension.to_ascii_uppercase())
    }

    /// Returns whether the target profile permits AVX2.
    pub fn profile_has_avx2(&self) -> bool {
        !self.is_unavailable("AVX2") && self.gated_extensions.contains_key("AVX2")
    }

    /// Checks if host CPU at runtime supports AVX2 (CPUID + OSXSAVE + XGETBV).
    #[inline]
    pub fn runtime_host_has_avx2() -> bool {
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            std::is_x86_feature_detected!("avx2")
        }
        #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
        {
            false
        }
    }

    /// Evaluates eligibility for AVX2 execution, returning whether it should be used
    /// along with a verifiable capability provenance audit record.
    pub fn check_avx2_eligibility(
        &self,
        mode: VectorMode,
        element_count: usize,
    ) -> (bool, CapabilityProvenance) {
        if self.is_unavailable("AVX2") {
            return (
                false,
                CapabilityProvenance {
                    extension: "AVX2".to_string(),
                    selected: false,
                    reason: "extension AVX2 is explicitly unavailable in target CPU profile"
                        .to_string(),
                },
            );
        }

        match mode {
            VectorMode::ForcedScalar => (
                false,
                CapabilityProvenance {
                    extension: "AVX2".to_string(),
                    selected: false,
                    reason: "forced scalar mode requested by caller".to_string(),
                },
            ),
            VectorMode::ForcedAvx2 => {
                if !self.profile_has_avx2() {
                    (
                        false,
                        CapabilityProvenance {
                            extension: "AVX2".to_string(),
                            selected: false,
                            reason:
                                "forced AVX2 requested but target profile does not advertise AVX2"
                                    .to_string(),
                        },
                    )
                } else if !Self::runtime_host_has_avx2() {
                    (
                        false,
                        CapabilityProvenance {
                            extension: "AVX2".to_string(),
                            selected: false,
                            reason: "forced AVX2 requested but runtime host lacks AVX2 or OSXSAVE/XGETBV support"
                                .to_string(),
                        },
                    )
                } else {
                    (
                        true,
                        CapabilityProvenance {
                            extension: "AVX2".to_string(),
                            selected: true,
                            reason: "forced AVX2 mode verified against target profile and host CPUID+XGETBV"
                                .to_string(),
                        },
                    )
                }
            }
            VectorMode::Auto => {
                if !self.profile_has_avx2() {
                    (
                        false,
                        CapabilityProvenance {
                            extension: "AVX2".to_string(),
                            selected: false,
                            reason: "target profile does not advertise AVX2".to_string(),
                        },
                    )
                } else if !Self::runtime_host_has_avx2() {
                    (
                        false,
                        CapabilityProvenance {
                            extension: "AVX2".to_string(),
                            selected: false,
                            reason: "runtime host CPUID/OSXSAVE does not support AVX2".to_string(),
                        },
                    )
                } else if element_count < AVX2_CROSSOVER_THRESHOLD {
                    (
                        false,
                        CapabilityProvenance {
                            extension: "AVX2".to_string(),
                            selected: false,
                            reason: format!(
                                "buffer length {element_count} is below vector crossover threshold {AVX2_CROSSOVER_THRESHOLD}"
                            ),
                        },
                    )
                } else {
                    (
                        true,
                        CapabilityProvenance {
                            extension: "AVX2".to_string(),
                            selected: true,
                            reason: format!(
                                "auto AVX2 selected: profile permits AVX2, host CPUID+XGETBV verified, length {element_count} >= threshold {AVX2_CROSSOVER_THRESHOLD}"
                            ),
                        },
                    )
                }
            }
        }
    }
}
