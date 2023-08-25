use core::hint::black_box;
use core::time::Duration;
use std::collections::{HashMap, HashSet};

use criterion::measurement::Measurement;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use hashmap::open_addressing::{linear_probing, quadratic_probing, robin_hood};
use rand::seq::IteratorRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

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

fn insert<M: Measurement>(c: &mut Criterion<M>) {
    let mut g = c.benchmark_group(format!("insert_new_{}", MEASUREMENT_KIND));

    macro_rules! bench {
        (new $name:expr, $count:expr, $keys:expr, $($map:tt)*) => {
            g.bench_with_input(BenchmarkId::new($name, $count), &$count, |b, _i| {
                b.iter(|| {
                    let mut map = $($map)*::new();
                    for x in $keys {
                        map.insert(x, x);
                    }
                    map
                })
            });
        };
        (lf $name:expr, $count:expr, $keys:expr, $lf:expr, $($map:tt)*) => {
            g.bench_with_input(BenchmarkId::new(format!("{}_{}", $name, $lf), $count), &$count, |b, _i| {
                b.iter(|| {
                    let mut map = $($map)*::with_load_factor($lf);
                    for x in $keys {
                        map.insert(x, x);
                    }
                    map
                })
            });
        };
    }
    let mut count = 1000;
    for _ in 0..40 {
        let keys = gen_unique_keys_int(count, true, i32::MAX / 2)
            .into_iter()
            .collect::<Vec<_>>();
        let keys = keys.iter().copied();
        bench!(new "std", count, keys.clone(), HashMap);
        for lf in [0.7, 0.9, 0.99] {
            bench!(
                lf "linear_probing",
                count,
                keys.clone(),
                lf,
                linear_probing::HashMap
            );
            bench!(
                lf "quadratic_probing",
                count,
                keys.clone(),
                lf,
                quadratic_probing::HashMap
            );
            bench!(lf "robin_hood", count, keys.clone(), lf, robin_hood::HashMap);
        }

        bench!(
            new "chaining_vecs",
            count,
            keys.clone(),
            hashmap::chaining::vecs::HashMap
        );
        count = (count as f64 * 1.05) as usize;
    }
}

macro_rules! bench_get {
    (new $g:expr, $name:expr, $count:expr, $keys:expr,  $access_keys:expr, $($map:tt)*) => {
        let mut map = $($map)*::with_capacity($count);
        for x in $keys {
            map.insert(x, x);
        }

        $g.bench_with_input(BenchmarkId::new($name, $count), &$count, |b, _c| {
            b.iter(|| {
                for k in $access_keys.iter() {
                    black_box(map.get(black_box(k)));
                }
            })
        });

    };
    (lf $g:expr, $name:expr, $count:expr, $keys:expr,  $access_keys:expr,  $lf:expr, $($map:tt)*) => {
        let mut map = $($map)*::with_capacity_and_load_factor($count, $lf);
        for x in $keys {
            map.insert(x, x);
        }

        $g.bench_with_input(BenchmarkId::new(format!("{}_{}", $name, $lf), $count), &$count, |b, _c| {
            b.iter(|| {
                for k in $access_keys.iter() {
                    black_box(map.get(black_box(k)));
                }
            })
        });

    };
}

fn get<M: Measurement>(c: &mut Criterion<M>) {
    let mut g = c.benchmark_group(format!("get_{}", MEASUREMENT_KIND));
    g.sampling_mode(criterion::SamplingMode::Flat);

    let mut count = 1000;
    for _ in 0..40 {
        let keys = gen_unique_keys_int(count, true, i32::MAX / 2);
        let keys = keys.iter().copied();
        let access_keys = sample_nonoverlapping_keys_valid(keys.clone(), count);

        bench_get!(new g, "std", count, keys.clone(), access_keys, HashMap);
        for lf in [0.7, 0.9, 0.99] {
            bench_get!(lf
                g,
                "linear_probing",
                count,
                keys.clone(),
                access_keys,
                lf,
                linear_probing::HashMap
            );
            bench_get!(lf
                g,
                "quadratic_probing",
                count,
                keys.clone(),
                access_keys,
                lf,
                quadratic_probing::HashMap
            );
            bench_get!(lf
                g,
                "robin_hood",
                count,
                keys.clone(),
                access_keys,
                lf,
                robin_hood::HashMap
            );
        }
        bench_get!(new
            g,
            "chaining_vecs",
            count,
            keys.clone(),
            access_keys,
            hashmap::chaining::vecs::HashMap
        );
        count = (count as f64 * 1.05) as usize;
    }
}

fn get_non_existing<M: Measurement>(c: &mut Criterion<M>) {
    let mut g = c.benchmark_group(format!("get_non_existing_{}", MEASUREMENT_KIND));

    let mut count = 1000;
    for _ in 0..40 {
        let keys = gen_unique_keys_int(count, true, i32::MAX / 2);
        let access_keys = sample_nonoverlapping_keys_invalid(&keys, count);
        let keys = keys.iter().copied();

        bench_get!(new g, "std", count, keys.clone(), access_keys, HashMap);
        for lf in [0.7, 0.9, 0.99] {
            bench_get!(lf
                g,
                "linear_probing",
                count,
                keys.clone(),
                access_keys,
                lf,
                linear_probing::HashMap
            );
            bench_get!(lf
                g,
                "quadratic_probing",
                count,
                keys.clone(),
                access_keys,
                lf,
                quadratic_probing::HashMap
            );
            bench_get!(lf
                g,
                "robin_hood",
                count,
                keys.clone(),
                access_keys,
                lf,
                robin_hood::HashMap
            );
        }
        bench_get!(new
            g,
            "chaining_vecs",
            count,
            keys.clone(),
            access_keys,
            hashmap::chaining::vecs::HashMap
        );
        count = (count as f64 * 1.05) as usize;
    }
}

fn remove<M: Measurement>(c: &mut Criterion<M>) {
    let mut g = c.benchmark_group(format!("remove_{}", MEASUREMENT_KIND));

    macro_rules! bench {
        ($name:expr, $count:expr, $keys:expr, $access_keys:expr, $($map:tt)*) => {
            let mut map = $($map)*::with_capacity($count);
            for x in $keys {
                map.insert(x, x);
            }

            g.bench_with_input(BenchmarkId::new($name, $count), &$count, |b, _i| {
                b.iter_batched_ref(
                    || map.clone(),
                    |map| {
                        for k in $access_keys.iter() {
                            black_box(map.remove(black_box(k)));
                        }
                    },
                    criterion::BatchSize::SmallInput,
                );
            });
        };
        (lf $name:expr, $count:expr, $keys:expr, $access_keys:expr, $lf:expr, $($map:tt)*) => {
            let mut map = $($map)*::with_capacity_and_load_factor($count, $lf);
            for x in $keys {
                map.insert(x, x);
            }

            g.bench_with_input(BenchmarkId::new(format!("{}_{}", $name, $lf), $count), &$count, |b, _i| {
                b.iter_batched_ref(
                    || map.clone(),
                    |map| {
                        for k in $access_keys.iter() {
                            black_box(map.remove(black_box(k)));
                        }
                    },
                    criterion::BatchSize::SmallInput,
                );
            });
        };
    }

    let mut count = 1000;
    for _ in 0..40 {
        let keys = gen_unique_keys_int(count, true, i32::MAX / 2);
        let keys = keys.iter().copied();
        let access_keys = sample_nonoverlapping_keys_valid(keys.clone(), count);

        bench!("std", count, keys.clone(), access_keys, HashMap);
        for lf in [0.7, 0.9, 0.99] {
            bench!(
                lf
                "linear_probing",
                count,
                keys.clone(),
                access_keys,
                lf,
                linear_probing::HashMap
            );
            bench!(
                lf
                "quadratic_probing",
                count,
                keys.clone(),
                access_keys,
                lf,
                quadratic_probing::HashMap
            );
            bench!(lf
                "robin_hood",
                count,
                keys.clone(),
                access_keys,
                lf,
                robin_hood::HashMap
            );
        }
        bench!(
            "chaining_vecs",
            count,
            keys.clone(),
            access_keys,
            hashmap::chaining::vecs::HashMap
        );
        count = (count as f64 * 1.05) as usize;
    }
}

pub fn gen_unique_keys_int(count: usize, random: bool, key_max: i32) -> HashSet<i32> {
    let mut set = HashSet::with_capacity(count);
    if random {
        let mut rng = ChaCha8Rng::seed_from_u64(123);
        let unique_keys = rand::seq::index::sample(&mut rng, key_max as usize, count);
        set.extend(unique_keys.into_iter().map(|a| a as i32));
    } else {
        set.extend((0..count).map(|a| a as i32));
    }

    assert_eq!(set.len(), count);
    set
}

pub fn sample_nonoverlapping_keys_valid<T>(keys: impl Iterator<Item = T>, count: usize) -> Vec<T>
where
    T: Clone,
{
    let mut index_gen = rand_chacha::ChaCha8Rng::seed_from_u64(321);
    keys.choose_multiple(&mut index_gen, count)
}

pub fn sample_nonoverlapping_keys_invalid(keys: &HashSet<i32>, count: usize) -> HashSet<i32> {
    let mut set = HashSet::with_capacity(count);
    let mut rng = ChaCha8Rng::seed_from_u64(456);

    loop {
        let key: i32 = rng.gen();
        if keys.contains(&key) {
            continue;
        }
        set.insert(key);

        if set.len() == count {
            break;
        }
    }

    assert_eq!(set.len(), count);
    set
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_millis(1000))
        .with_measurement(create_measurement())
        ;
    targets = get, get_non_existing, insert, remove
);
criterion_main!(benches);
