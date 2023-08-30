use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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

struct Cmps<T> {
    inner: T,
    cmps: Rc<AtomicU64>,
}

impl<T> Cmps<T> {
    fn cmps(&self) -> u64 {
        self.cmps.load(Ordering::SeqCst)
    }
}

impl<T> PartialEq for Cmps<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.cmps.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner == other.inner
    }
}

impl<T> PartialOrd for Cmps<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.cmps.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.partial_cmp(&other.inner)
    }
}

impl<T> Eq for Cmps<T> where T: Eq {}
impl<T> Ord for Cmps<T>
where
    T: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cmps.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.cmp(&other.inner)
    }
}

fn gen_random(count: usize, key_max: i32) -> Vec<Cmps<i32>> {
    let counter = Rc::new(AtomicU64::new(0));
    let mut vec = Vec::with_capacity(count);
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    for _ in 0..count {
        let val = rng.gen_range(0..key_max);
        vec.push(Cmps {
            inner: val,
            cmps: Rc::clone(&counter),
        })
    }
    assert_eq!(vec.len(), count);
    vec
}

#[test]
#[ignore = "not a test, prints metrics"]
fn print_metrics() {
    fn print(name: &str, sort: fn(&mut [Cmps<i32>])) {
        for count in [100, 1000, 10000] {
            let mut data = gen_random(count, i32::MAX);
            sort(&mut data);
            println!("{name}_{count} = {}", data[0].cmps());
        }
    }

    macro_rules! print_metrics {
        ($($name:ident),+ $(,)?) => {
            $(
                print(stringify!($name), $name);
            )+
        };
    }

    print_metrics!(
        bubble_sort,
        insertion_sort,
        insertion_sort2,
        selection_sort,
        selection_sort2,
        merge_sort,
        // merge_sort_copy,
        heapsort,
        quicksort_hoare,
        quicksort_lomuto,
        std_sort,
        std_sort_unstable,
    );
}
