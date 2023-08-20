use core::hint::black_box;
use std::collections::{HashMap, HashSet};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use hashmap::open_addressing::{linear_probing, robin_hood};
use rand::distributions::uniform::SampleUniform;
use rand::seq::IteratorRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

fn insert(c: &mut Criterion) {
    let mut g = c.benchmark_group("insert_new");

    macro_rules! bench {
        ($name:expr, $count:expr, $keys:expr, $($map:tt)*) => {
            g.bench_function(BenchmarkId::new($name, $count), |b| {
                b.iter(|| {
                    let mut map = $($map)*::new();
                    for x in $keys {
                        map.insert(x, x);
                    }
                    map
                })
            });
        };
    }

    for count in [100, 10_000] {
        let keys = gen_unique_keys_int(count, true, i32::MAX / 2)
            .into_iter()
            .collect::<Vec<_>>();
        let keys = keys.iter().copied();
        bench!("std", count, keys.clone(), HashMap);
        bench!(
            "linear_probing",
            count,
            keys.clone(),
            linear_probing::HashMap
        );
        bench!("robin_hood", count, keys.clone(), robin_hood::HashMap);
        bench!(
            "chaining_vecs",
            count,
            keys.clone(),
            hashmap::chaining::vecs::HashMap
        );
    }
}

fn get(c: &mut Criterion) {
    let mut g = c.benchmark_group("get");

    macro_rules! bench {
        ($name:expr, $count:expr, $keys:expr, $access_keys:expr, $($map:tt)*) => {
            let mut map = $($map)*::new();
            for x in $keys {
                map.insert(x, x);
            }

            g.bench_function(BenchmarkId::new($name, $count), |b| {
                b.iter(|| {
                    for k in $access_keys.iter() {
                        black_box(map.get(black_box(k)));
                    }
                })
            });

        };
    }

    for count in [1000, 10_000, 100_000] {
        let keys = gen_unique_keys_int(count, true, i32::MAX / 2);
        let keys = keys.iter().copied();
        let access_keys = sample_nonoverlapping_keys_valid(keys.clone(), count);

        bench!("std", count, keys.clone(), access_keys, HashMap);
        bench!(
            "linear_probing",
            count,
            keys.clone(),
            access_keys,
            linear_probing::HashMap
        );
        bench!(
            "robin_hood",
            count,
            keys.clone(),
            access_keys,
            robin_hood::HashMap
        );
        bench!(
            "chaining_vecs",
            count,
            keys.clone(),
            access_keys,
            hashmap::chaining::vecs::HashMap
        );
    }
}

fn remove(c: &mut Criterion) {
    let mut g = c.benchmark_group("remove");

    macro_rules! bench {
        ($name:expr, $count:expr, $keys:expr, $access_keys:expr, $($map:tt)*) => {
            let mut map = $($map)*::new();
            for x in $keys {
                map.insert(x, x);
            }

            g.bench_function(BenchmarkId::new($name, $count), |b| {
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

    for count in [1000, 10_000] {
        let keys = gen_unique_keys_int(count, true, i32::MAX / 2);
        let keys = keys.iter().copied();
        let access_keys = sample_nonoverlapping_keys_valid(keys.clone(), count);

        bench!("std", count, keys.clone(), access_keys, HashMap);
        bench!(
            "linear_probing",
            count,
            keys.clone(),
            access_keys,
            linear_probing::HashMap
        );
        bench!(
            "robin_hood",
            count,
            keys.clone(),
            access_keys,
            robin_hood::HashMap
        );
        bench!(
            "chaining_vecs",
            count,
            keys.clone(),
            access_keys,
            hashmap::chaining::vecs::HashMap
        );
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

criterion_group!(benches, insert, get, remove);
criterion_main!(benches);
