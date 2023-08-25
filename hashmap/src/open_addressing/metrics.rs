use core::borrow::Borrow;
use core::hash::Hash;
use hdrhistogram::Histogram;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashSet;

pub(super) trait MapMetrics<K, V> {
    /// Return (key, value, number of probes)
    ///
    /// Note that number of probes starts from 0, so if you get it at preferred index then it's 0
    fn get_with_metrics<Q>(&self, key: &Q) -> Option<(&K, &V, usize)>
    where
        Q: Eq + Hash,
        K: Borrow<Q>;
    fn len(&self) -> usize;
    fn cap(&self) -> usize;
    fn load_factor(&self) -> f64;
    fn name(&self) -> &'static str;
}

fn gen_unique_keys_int(count: usize, random: bool, key_max: u64) -> HashSet<u64> {
    let mut set = HashSet::with_capacity(count);
    if random {
        let mut rng = ChaCha8Rng::seed_from_u64(123);
        let unique_keys = rand::seq::index::sample(&mut rng, key_max as usize, count);
        set.extend(unique_keys.into_iter().map(|a| a as u64));
    } else {
        set.extend((0..count).map(|a| a as u64));
    }

    assert_eq!(set.len(), count);
    set
}

#[test]
#[ignore = "not really a test but prints some metrics about different maps"]
fn metrics() {
    struct Stats {
        probes: Histogram<u64>,
    }

    fn calc_stats<'a, K: 'a, V>(
        keys: impl Iterator<Item = &'a K>,
        map: &impl MapMetrics<K, V>,
    ) -> Stats
    where
        K: Eq + Hash,
    {
        let mut probes_hist = Histogram::new(3).unwrap();

        for key in keys {
            let (_, _, probes) = match map.get_with_metrics(key) {
                Some(v) => v,
                None => {
                    continue;
                }
            };
            probes_hist.record(probes as u64).unwrap();
        }

        Stats {
            probes: probes_hist,
        }
    }

    fn print_stats<'a, K: 'a, V>(keys: impl Iterator<Item = &'a K>, map: &impl MapMetrics<K, V>)
    where
        K: Eq + Hash,
    {
        #[derive(Debug)]
        struct StatsPrint {
            min: u64,
            p10: u64,
            p25: u64,
            p50: u64,
            p75: u64,
            p90: u64,
            max: u64,
            mean: f64,
            std: f64,
        }

        impl StatsPrint {
            fn new(stats: &Histogram<u64>) -> Self {
                Self {
                    min: stats.min(),
                    p10: stats.value_at_quantile(0.10),
                    p25: stats.value_at_quantile(0.25),
                    p50: stats.value_at_quantile(0.5),
                    p75: stats.value_at_quantile(0.75),
                    p90: stats.value_at_quantile(0.9),
                    max: stats.max(),
                    mean: stats.mean(),
                    std: stats.stdev(),
                }
            }
        }

        let stats = calc_stats(keys, map);
        println!(
            "\n{}\nmetrics @ load factor={}/{}={:.3}\n  probes={:#?}",
            map.name(),
            map.len(),
            map.cap(),
            map.load_factor(),
            StatsPrint::new(&stats.probes)
        );
    }

    let cap = 2_usize.pow(17);
    let count_at_0999 = (cap as f64 * 0.999) as usize;
    let count_at_099 = (cap as f64 * 0.99) as usize;
    let count_at_090 = (cap as f64 * 0.90) as usize;
    let keys = gen_unique_keys_int(count_at_0999, true, u64::MAX / 2);
    let load_factor = 0.999999999;
    let mut rh = super::robin_hood::HashMap::with_capacity_and_load_factor(cap - 1, load_factor);
    let mut lin =
        super::linear_probing::HashMap::with_capacity_and_load_factor(cap - 1, load_factor);
    let mut quad =
        super::quadratic_probing::HashMap::with_capacity_and_load_factor(cap - 1, load_factor);
    assert_eq!(rh.cap(), cap);
    assert_eq!(lin.cap(), cap);
    assert_eq!(quad.cap(), cap);

    for k in keys.iter().copied() {
        rh.insert(k, k);
        lin.insert(k, k);
        quad.insert(k, k);
        if rh.len() == count_at_090 || rh.len() == count_at_099 {
            print_stats(keys.iter(), &lin);
            print_stats(keys.iter(), &rh);
            print_stats(keys.iter(), &quad);
        }
    }

    print_stats(keys.iter(), &lin);
    print_stats(keys.iter(), &rh);
    print_stats(keys.iter(), &quad);
}
