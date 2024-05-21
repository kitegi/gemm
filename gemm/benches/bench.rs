use std::time::Duration;

use aligned_vec::{avec, AVec};
use diol::{
    config::{MaxTime, MinTime},
    prelude::*,
};
use gemm::*;
use num_traits::One;
use regex::Regex;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Layout {
    Col,
    Row,
}

fn make_data<T: Copy + One>(
    layout: Layout,
    m: usize,
    n: usize,
    reg: usize,
) -> (isize, isize, AVec<T>) {
    let val = T::one();
    match layout {
        Layout::Col => (
            1,
            m.next_multiple_of(reg) as isize,
            avec![val; n * m.next_multiple_of(reg)],
        ),
        Layout::Row => (
            n.next_multiple_of(reg) as isize,
            1,
            avec![val; m * n.next_multiple_of(reg)],
        ),
    }
}

fn bench_gemm<T: One + Copy + 'static>(
    bencher: Bencher,
    list![par, dst, lhs, rhs, m, n, k]: List![
        Parallelism,
        Layout,
        Layout,
        Layout,
        usize,
        usize,
        usize
    ],
) {
    let reg = 64 / core::mem::size_of::<T>();

    let (dst_rs, dst_cs, mut dst) = make_data::<T>(dst, m, n, reg);
    let (lhs_rs, lhs_cs, mut lhs) = make_data::<T>(lhs, m, k, reg);
    let (rhs_rs, rhs_cs, mut rhs) = make_data::<T>(rhs, k, n, reg);

    lhs.fill(unsafe { core::mem::zeroed() });
    rhs.fill(unsafe { core::mem::zeroed() });
    dst.fill(unsafe { core::mem::zeroed() });

    bencher.bench(|| {
        unsafe {
            gemm(
                m,
                n,
                k,
                dst.as_mut_ptr(),
                dst_cs,
                dst_rs,
                true,
                lhs.as_ptr(),
                lhs_cs,
                lhs_rs,
                rhs.as_ptr(),
                rhs_cs,
                rhs_rs,
                T::one(),
                T::one(),
                false,
                false,
                false,
                par,
            )
        };
    })
}

fn args() -> Vec<List![Parallelism, Layout, Layout, Layout, usize, usize, usize]> {
    use itertools::Itertools;
    let pow2 = |i| 1usize << i;
    let halfway = |i| 3usize << (i - 1);
    itertools::iproduct!(
        [].into_iter()
            .chain((5..13).map(pow2).map(|n| (n, n, n)))
            .chain((5..13).map(halfway).map(|n| (n, n, n)))
            .sorted_unstable(),
        [Parallelism::Rayon(0), Parallelism::None],
        [Layout::Col, Layout::Row],
        [Layout::Col, Layout::Row],
        [Layout::Col, Layout::Row]
    )
    .map(|((m, n, k), par, dst, lhs, rhs)| list![par, dst, lhs, rhs, m, n, k])
    .collect()
}

fn main() {
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Clap {
        #[arg(long)]
        bench: bool,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        arg: Option<String>,
    }

    let clap = Clap::parse();
    let mut config = BenchConfig::default();
    if let Some(name) = &clap.name {
        config.func_filter = Some(Regex::new(name).unwrap());
    }
    if let Some(arg) = &clap.arg {
        config.arg_filter = Some(Regex::new(arg).unwrap());
    }
    config.min_time = MinTime(Duration::from_secs(1));
    config.max_time = MaxTime(Duration::from_secs(1));

    gemm::set_wasm_simd128(true);

    let modifiers = [1];

    {
        let mut bench = Bench::new(config.clone());
        bench.register(bench_gemm::<f32>, args());

        for modifier in modifiers {
            gemm::set_threading_threshold(gemm::DEFAULT_THREADING_THRESHOLD / modifier);
            bench.run().unwrap();
        }
    }
    {
        let mut bench = Bench::new(config.clone());
        bench.register(bench_gemm::<f64>, args());
        for modifier in modifiers {
            gemm::set_threading_threshold(gemm::DEFAULT_THREADING_THRESHOLD / modifier);
            bench.run().unwrap();
        }
    }
    {
        let mut bench = Bench::new(config.clone());
        bench.register(bench_gemm::<c32>, args());
        for modifier in modifiers {
            gemm::set_threading_threshold(gemm::DEFAULT_THREADING_THRESHOLD / modifier);
            bench.run().unwrap();
        }
    }
    {
        let mut bench = Bench::new(config.clone());
        bench.register(bench_gemm::<c64>, args());
        for modifier in modifiers {
            gemm::set_threading_threshold(gemm::DEFAULT_THREADING_THRESHOLD / modifier);
            bench.run().unwrap();
        }
    }
}
