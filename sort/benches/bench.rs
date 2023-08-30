use core::time::Duration;

use criterion::measurement::Measurement;
use criterion::{
    criterion_group, criterion_main, AxisScale, BenchmarkGroup, BenchmarkId, Criterion,
    PlotConfiguration,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sort::bubble_sort::bubble_sort;
use sort::heapsort::heapsort;
use sort::insertion_sort::{insertion_sort, insertion_sort2};
use sort::merge_sort::{merge_sort, merge_sort_copy};
use sort::quicksort::{quicksort_hoare, quicksort_lomuto};
use sort::selection_sort::{selection_sort, selection_sort2};

fn std_sort<T: Ord>(slice: &mut [T]) {
    slice.sort()
}

fn std_sort_unstable<T: Ord>(slice: &mut [T]) {
    slice.sort_unstable()
}

pub fn gen_random_ints(count: usize, key_max: i32) -> Vec<i32> {
    let mut vec = Vec::with_capacity(count);
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    for _ in 0..count {
        vec.push(rng.gen_range(0..key_max))
    }
    assert_eq!(vec.len(), count);
    vec
}

pub fn gen_ascending_ints_maybe_duplicates(count: usize, key_max: i32) -> Vec<i32> {
    let mut vec = Vec::with_capacity(count);
    let mut rng = ChaCha8Rng::seed_from_u64(2);
    for _ in 0..count {
        vec.push(rng.gen_range(0..key_max))
    }
    assert_eq!(vec.len(), count);
    vec.sort();
    vec
}

pub fn gen_ascending_ints_no_duplicates(count: usize, key_max: i32) -> Vec<i32> {
    assert!(count < key_max as usize);
    let mut vec = Vec::with_capacity(count);
    let mut rng = ChaCha8Rng::seed_from_u64(3);
    let a = rand::seq::index::sample(&mut rng, key_max as usize, count);
    vec.extend(a.into_iter().map(|a| a as i32));
    assert_eq!(vec.len(), count);
    vec.sort();
    vec
}

pub fn gen_descending_ints_maybe_duplicates(count: usize, key_max: i32) -> Vec<i32> {
    let mut vec = Vec::with_capacity(count);
    let mut rng = ChaCha8Rng::seed_from_u64(4);
    for _ in 0..count {
        vec.push(rng.gen_range(0..key_max))
    }
    assert_eq!(vec.len(), count);
    vec.sort_by(|a, b| b.cmp(a));
    vec
}

pub fn gen_descending_ints_no_duplicates(count: usize, key_max: i32) -> Vec<i32> {
    assert!(count < key_max as usize);
    let mut vec = Vec::with_capacity(count);
    let mut rng = ChaCha8Rng::seed_from_u64(5);
    let a = rand::seq::index::sample(&mut rng, key_max as usize, count);
    vec.extend(a.into_iter().map(|a| a as i32));
    assert_eq!(vec.len(), count);
    vec.sort_by(|a, b| b.cmp(a));
    vec
}

pub fn gen_equal(count: usize, key_max: i32) -> Vec<i32> {
    assert!(count < key_max as usize);
    vec![153; count]
}

macro_rules! select_measurement {
    (refcycles) => {
        pub const MEASUREMENT_KIND: &str = "refcycles";

        pub fn create_measurement() -> impl ::criterion::measurement::Measurement {
            ::criterion_perf_events::Perf::new(
                ::perfcnt::linux::PerfCounterBuilderLinux::from_hardware_event(
                    ::perfcnt::linux::HardwareEventType::RefCPUCycles,
                ),
            )
        }
    };
    (instructions) => {
        pub const MEASUREMENT_KIND: &str = "instructions";

        pub fn create_measurement() -> impl ::criterion::measurement::Measurement {
            ::criterion_perf_events::Perf::new(
                ::perfcnt::linux::PerfCounterBuilderLinux::from_hardware_event(
                    ::perfcnt::linux::HardwareEventType::Instructions,
                ),
            )
        }
    };
    (walltime) => {
        pub const MEASUREMENT_KIND: &str = "walltime";

        pub fn create_measurement() -> impl ::criterion::measurement::Measurement {
            ::criterion::measurement::WallTime
        }
    };
}

select_measurement!(walltime);

fn bench_group<M: Measurement>(
    c: &mut Criterion<M>,
    name: &str,
    gen_func: fn(usize, i32) -> Vec<i32>,
) {
    fn bench_one<M: Measurement>(
        g: &mut BenchmarkGroup<'_, M>,
        name: &str,
        count: usize,
        items: &Vec<i32>,
        sort: fn(&mut [i32]),
    ) {
        g.bench_with_input(BenchmarkId::new(name, count), &count, |b, _i| {
            b.iter_batched_ref(
                || items.clone(),
                |i| sort(i),
                criterion::BatchSize::SmallInput,
            )
        });
    }

    macro_rules! bench {
        ($g:expr, $count:expr, $vec:expr, $($sort:path),+ $(,)?) => {
           $(
               bench_one($g, stringify!($sort), $count, &$vec, $sort);
            )+
        };
    }

    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
    let mut g = c.benchmark_group(format!("{}_{}", name, MEASUREMENT_KIND));
    g.plot_config(plot_config.clone());

    for count in [10, 100, 1_000, 10_000] {
        let vec = gen_func(count, i32::MAX);
        bench!(
            &mut g,
            count,
            vec,
            bubble_sort,
            insertion_sort,
            insertion_sort2,
            selection_sort,
            selection_sort2,
            merge_sort,
            merge_sort_copy,
            heapsort,
            quicksort_hoare,
            quicksort_lomuto,
            std_sort,
            std_sort_unstable,
        );
    }
    g.finish();
}

fn bench<M: Measurement>(c: &mut Criterion<M>) {
    bench_group(c, "random", gen_random_ints);
    bench_group(c, "ascending", gen_ascending_ints_no_duplicates);
    bench_group(c, "descending", gen_descending_ints_no_duplicates);
    bench_group(c, "equal", gen_equal);
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(1))
        .warm_up_time(Duration::from_millis(100))
        .with_measurement(create_measurement())
        ;
    targets = bench
);
criterion_main!(benches);
