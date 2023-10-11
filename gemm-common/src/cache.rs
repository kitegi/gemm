use lazy_static::lazy_static;

#[derive(Default, Debug, Copy, Clone)]
pub struct CacheInfo {
    pub small_mc: bool,
    pub associativity: usize,
    pub cache_bytes: usize,
    pub cache_line_bytes: usize,
}

#[derive(Default, Debug, Copy, Clone)]
pub struct KernelParams {
    pub kc: usize,
    pub mc: usize,
    pub nc: usize,
}

#[cfg(target_arch = "aarch64")]
fn cache_info() -> Option<[CacheInfo; 3]> {
    #[cfg(not(miri))]
    {
        use sysctl::{Ctl, Sysctl};
        let l1: usize = Ctl::new("hw.l1dcachesize")
            .ok()?
            .value_string()
            .ok()?
            .parse::<usize>()
            .unwrap()
            / 2;
        let l2: usize = Ctl::new("hw.l2dcachesize")
            .ok()?
            .value_string()
            .ok()?
            .parse::<usize>()
            .unwrap()
            / 2;

        Some([
            CacheInfo {
                small_mc: false,
                associativity: 8,
                cache_bytes: l1,
                cache_line_bytes: 128,
            },
            CacheInfo {
                small_mc: false,
                associativity: 8,
                cache_bytes: l2,
                cache_line_bytes: 128,
            },
            CacheInfo {
                small_mc: false,
                associativity: 8,
                cache_bytes: 0,
                cache_line_bytes: 128,
            },
        ])
    }

    #[cfg(miri)]
    None
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn cache_info() -> Option<[CacheInfo; 3]> {
    #[cfg(not(miri))]
    {
        use raw_cpuid::CpuId;
        let cpuid = CpuId::new();

        if let Some(vf) = cpuid.get_vendor_info() {
            let vf = vf.as_str();
            if vf == "GenuineIntel" {
                if let Some(cparams) = cpuid.get_cache_parameters() {
                    // not sure why, intel cpus seem to prefer smaller mc
                    let mut info = [CacheInfo {
                        cache_bytes: 0,
                        associativity: 0,
                        cache_line_bytes: 64,
                        small_mc: true,
                    }; 3];

                    for cache in cparams {
                        use raw_cpuid::CacheType::*;
                        match cache.cache_type() {
                            Null | Instruction | Reserved => continue,
                            Data | Unified => {
                                let level = cache.level() as usize;
                                let associativity = cache.associativity();
                                let nsets = cache.sets();
                                let cache_line_bytes = cache.coherency_line_size();
                                if level > 0 && level < 4 {
                                    let info = &mut info[level - 1];
                                    info.cache_line_bytes = cache_line_bytes;
                                    info.associativity = associativity;
                                    info.cache_bytes = associativity * nsets * cache_line_bytes;
                                }
                            }
                        }
                    }
                    return Some(info);
                }
            }

            if vf == "AuthenticAMD" {
                if let Some(l1) = cpuid.get_l1_cache_and_tlb_info() {
                    if let Some(l23) = cpuid.get_l2_l3_cache_and_tlb_info() {
                        let compute_info = |associativity: raw_cpuid::Associativity,
                                            cache_kb: usize,
                                            cache_line_bytes: u8|
                         -> CacheInfo {
                            let cache_bytes = cache_kb as usize * 1024;
                            let cache_line_bytes = cache_line_bytes as usize;

                            use raw_cpuid::Associativity::*;
                            let associativity = match associativity {
                                Unknown | Disabled => {
                                    return CacheInfo {
                                        associativity: 0,
                                        cache_bytes: 0,
                                        cache_line_bytes: 64,
                                        small_mc: false,
                                    }
                                }
                                FullyAssociative => cache_bytes / cache_line_bytes,
                                DirectMapped => 1,
                                NWay(n) => n as usize,
                            };

                            CacheInfo {
                                associativity,
                                cache_bytes,
                                cache_line_bytes,
                                small_mc: false,
                            }
                        };
                        return Some([
                            compute_info(
                                l1.dcache_associativity(),
                                l1.dcache_size() as usize,
                                l1.dcache_line_size(),
                            ),
                            compute_info(
                                l23.l2cache_associativity(),
                                l23.l2cache_size() as usize,
                                l23.l2cache_line_size(),
                            ),
                            compute_info(
                                l23.l3cache_associativity(),
                                l23.l3cache_size() as usize * 512,
                                l23.l3cache_line_size(),
                            ),
                        ]);
                    }
                }
            }
        }
        None
    }

    #[cfg(miri)]
    None
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
fn cache_info() -> Option<[CacheInfo; 3]> {
    None
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
static CACHE_INFO_DEFAULT: [CacheInfo; 3] = [
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 32 * 1024, // 32KiB
        cache_line_bytes: 64,
    },
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 256 * 1024, // 256KiB
        cache_line_bytes: 64,
    },
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 2 * 1024 * 1024, // 2MiB
        cache_line_bytes: 64,
    },
];

#[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
static CACHE_INFO_DEFAULT: [CacheInfo; 3] = [
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 64 * 1024, // 64KiB
        cache_line_bytes: 64,
    },
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 512 * 1024, // 512KiB
        cache_line_bytes: 64,
    },
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 4 * 1024 * 1024, // 4MiB
        cache_line_bytes: 64,
    },
];

#[cfg(not(any(
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "x86",
    target_arch = "x86_64"
)))]
static CACHE_INFO_DEFAULT: [CacheInfo; 3] = [
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 16 * 1024, // 16KiB
        cache_line_bytes: 64,
    },
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 512 * 1024, // 512KiB
        cache_line_bytes: 64,
    },
    CacheInfo {
        small_mc: false,
        associativity: 8,
        cache_bytes: 1024 * 1024, // 1MiB
        cache_line_bytes: 64,
    },
];

lazy_static! {
    pub static ref CACHE_INFO: [CacheInfo; 3] = cache_info().unwrap_or(CACHE_INFO_DEFAULT);
}

#[inline]
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let rem = a % b;
        a = b;
        b = rem;
    }
    a
}

#[inline]
pub fn div_ceil(a: usize, b: usize) -> usize {
    let d = a / b;
    let r = a % b;
    if r > 0 && b > 0 {
        d + 1
    } else {
        d
    }
}
#[inline]
fn round_down(a: usize, b: usize) -> usize {
    a / b * b
}

pub fn kernel_params(
    m: usize,
    n: usize,
    k: usize,
    mr: usize,
    nr: usize,
    sizeof: usize,
) -> KernelParams {
    if m == 0 || n == 0 || k == 0 {
        return KernelParams {
            kc: k,
            mc: m,
            nc: n,
        };
    }

    let info = *CACHE_INFO;

    let l1_cache_bytes = info[0].cache_bytes.max(32 * 1024);
    let l2_cache_bytes = info[1].cache_bytes;
    let l3_cache_bytes = info[2].cache_bytes;

    let l1_line_bytes = info[0].cache_line_bytes.max(64);

    let l1_assoc = info[0].associativity.max(2);
    let l2_assoc = info[1].associativity.max(2);
    let l3_assoc = info[2].associativity.max(2);

    let l1_n_sets = l1_cache_bytes / (l1_line_bytes * l1_assoc);

    // requires
    // A micropanels must occupy different cache sets
    // so that loading a micropanel evicts the previous one
    // => byte stride must be multiple of n_sets×line_bytes
    //
    // => mr×kc×scalar_bytes == C_A × l1_line_bytes × l1_n_sets
    //
    // l1 must be able to hold A micropanel, B micropanel
    //
    // => C_A + C_B <= l1_assoc

    // a×n = b×m
    // find lcm of a, b
    // n = lcm / a = b/gcd(a,b)
    // m = lcm / b = a/gcd(a,b)

    let gcd = gcd(mr * sizeof, l1_line_bytes * l1_n_sets);
    let kc_0 = (l1_line_bytes * l1_n_sets) / gcd;
    let c_lhs = (mr * sizeof) / gcd;
    let c_rhs = (nr * kc_0 * sizeof) / (l1_line_bytes * l1_n_sets);
    let kc_multiplier = l1_assoc / (c_lhs + c_rhs);
    // let auto_kc = kc_0 * kc_multiplier;
    let auto_kc = (kc_0 * kc_multiplier.next_power_of_two()).max(512).min(k);
    let k_iter = div_ceil(k, auto_kc);
    let auto_kc = div_ceil(k, k_iter);

    // l2 cache must hold
    //  - B micropanel: nr×kc: assume 1 assoc degree
    //  - A macropanel: mc×kc
    // mc×kc×scalar_bytes
    let auto_mc = if l2_cache_bytes == 0 {
        panic!();
    } else {
        let rhs_micropanel_bytes = nr * auto_kc * sizeof;
        let rhs_l2_assoc = div_ceil(rhs_micropanel_bytes, l2_cache_bytes / l2_assoc);
        let lhs_l2_assoc = (l2_assoc - 1 - rhs_l2_assoc).max(1);

        let mc_from_lhs_l2_assoc = |lhs_l2_assoc: usize| -> usize {
            (lhs_l2_assoc * l2_cache_bytes) / (l2_assoc * sizeof * auto_kc)
        };

        let auto_mc = round_down(
            mc_from_lhs_l2_assoc(if info[1].small_mc {
                lhs_l2_assoc / 2 + 1
            } else {
                lhs_l2_assoc
            }),
            mr,
        );
        let m_iter = div_ceil(m, auto_mc);
        div_ceil(m, m_iter * mr) * mr
    };

    // l3 cache must hold
    //  - A macropanel: mc×kc: assume 1 assoc degree
    //  - B macropanel: nc×kc
    let auto_nc = if l3_cache_bytes == 0 {
        0
    } else {
        // let lhs_macropanel_bytes = auto_mc * auto_kc * sizeof;
        // let lhs_l3_assoc = div_ceil(lhs_macropanel_bytes, l3_cache_bytes / l3_assoc);
        let rhs_l3_assoc = l3_assoc - 1;
        let rhs_macropanel_max_bytes = (rhs_l3_assoc * l3_cache_bytes) / l3_assoc;

        let auto_nc = round_down(rhs_macropanel_max_bytes / (sizeof * auto_kc), nr);
        let n_iter = div_ceil(n, auto_nc);
        div_ceil(n, n_iter * nr) * nr
    };

    KernelParams {
        kc: auto_kc,
        mc: auto_mc,
        nc: auto_nc,
    }
}
