//! Hash map with Robin Hood hashing variant of linear probing

extern crate alloc as crate_alloc;

use core::alloc::Layout;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash, Hasher};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use core::{fmt, mem};
use std::collections::hash_map::RandomState;

use crate_alloc::alloc;

#[cfg(test)]
use super::metrics::MapMetrics;
use super::round_up_to_power_of_two;

type HashValue = u64;

#[derive(Debug, Clone)]
struct Bucket<K, V> {
    key: K,
    value: V,
    hash: HashValue,
}

pub struct HashMap<K, V> {
    buf: NonNull<Option<Bucket<K, V>>>,
    cap: usize,
    index_mask: usize,
    len: usize,
    hash_builder: RandomState,
    crit_load_factor: f64,
    marker: PhantomData<(K, V)>,
}

impl<K, V> Drop for HashMap<K, V> {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }

        for i in 0..self.cap {
            let it = unsafe { self.buf.as_ptr().add(i) };
            unsafe { ptr::drop_in_place(it) };
        }

        let layout = Self::layout(self.cap);
        unsafe { alloc::dealloc(self.buf.as_ptr().cast::<u8>(), layout) }
    }
}

impl<K, V> Clone for HashMap<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn clone(&self) -> Self {
        // TODO: improve it
        let mut s = Self {
            buf: NonNull::dangling(),
            cap: 0,
            index_mask: 0,
            len: 0,
            crit_load_factor: self.crit_load_factor,
            hash_builder: self.hash_builder.clone(),
            marker: self.marker,
        };
        s.grow_to(self.cap);
        for i in 0..self.cap {
            let it = unsafe { &*self.buf.as_ptr().add(i) };
            if let Some(bucket) = it {
                unsafe { s.insert_unchecked(bucket.clone()) };
            }
        }

        s
    }
}

impl<K, V> fmt::Debug for HashMap<K, V>
where
    K: fmt::Debug + Hash + Eq,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HashMap")
            .field("buf", &DebugHashMapBuf { map: self })
            .field("cap", &self.cap)
            .field("len", &self.len)
            .field("hash_builder", &self.hash_builder)
            .finish()
    }
}

struct DebugHashMapBuf<'a, K, V> {
    map: &'a HashMap<K, V>,
}

impl<'a, K, V> fmt::Debug for DebugHashMapBuf<'a, K, V>
where
    K: fmt::Debug + Hash + Eq,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();

        for i in 0..self.map.cap {
            let it = unsafe { &*self.map.buf.as_ptr().add(i) };
            let it = it.as_ref().map(|b| {
                let hash = self.map.hash_key(&b.key);
                let orig_index = self.map.preferred_index(hash);
                (
                    &b.key,
                    &b.value,
                    orig_index,
                    self.map.probe_len(orig_index, i),
                )
            });
            list.entry(&it);
        }

        list.finish()
    }
}

impl<K, V> HashMap<K, V> {
    const DEF_CRIT_LOAD_FACTOR: f64 = 0.7;
    const INITIAL_CAP: usize = 4;

    pub fn new() -> Self {
        Self::with_load_factor(Self::DEF_CRIT_LOAD_FACTOR)
    }

    pub fn with_load_factor(load_factor: f64) -> Self {
        Self::with_capacity_and_load_factor(0, load_factor)
    }

    /// Creates a new hash map with capacity to store at least `capacity` pairs
    /// without reallocation.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_load_factor(capacity, Self::DEF_CRIT_LOAD_FACTOR)
    }

    /// Creates a new hash map with capacity to store at least `capacity` pairs
    /// without reallocation.
    pub fn with_capacity_and_load_factor(capacity: usize, lf: f64) -> Self {
        let (buf, cap, index_mask) = if capacity > 0 {
            let capacity = (capacity as f64 / lf + 1.0) as usize;
            let capacity = round_up_to_power_of_two(capacity);
            debug_assert!(capacity.is_power_of_two());
            debug_assert!(capacity > 0);
            let new_buf = unsafe { Self::alloc_new_buf_initialized(capacity) };
            (new_buf, capacity, capacity - 1)
        } else {
            (NonNull::dangling(), 0, 0)
        };
        Self {
            buf,
            cap,
            index_mask,
            len: 0,
            hash_builder: RandomState::new(),
            crit_load_factor: lf,
            marker: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn probe_len(&self, orig_index: usize, actual_index: usize) -> usize {
        if actual_index < orig_index {
            // probe must wrap around
            (self.cap - orig_index) + actual_index
        } else {
            actual_index - orig_index
        }
    }

    fn preferred_index(&self, hash: u64) -> usize {
        debug_assert!(self.cap < isize::MAX as usize);
        debug_assert!(self.cap.is_power_of_two());
        // SAFETY: cap <= isize::MAX, hence the result after modulo must be < isize::MAX
        (hash & (self.index_mask as u64)) as usize
    }

    fn load_factor(&self) -> f64 {
        if self.cap == 0 {
            return f64::INFINITY;
        }

        self.len as f64 / self.cap as f64
    }

    fn layout(cap: usize) -> Layout {
        Layout::array::<Option<Bucket<K, V>>>(cap).unwrap()
    }
}

impl<K, V> HashMap<K, V>
where
    K: Hash + Eq,
{
    pub fn insert(&mut self, key: K, value: V) -> Option<(K, V)> {
        if self.load_factor() > self.crit_load_factor {
            self.grow()
        }

        debug_assert!(self.len < self.cap);
        let hash = self.hash_key(&key);
        unsafe { self.insert_unchecked(Bucket { key, value, hash }) }
    }

    /// # SAFETY
    ///
    /// * Self must have the capacity for 1 more item
    ///   (ideally we would also not exceed `load_factor > Self::CRIT_LOAD_FACTOR`
    ///   but that's not a safety requirement)
    unsafe fn insert_unchecked(&mut self, mut bucket: Bucket<K, V>) -> Option<(K, V)> {
        let mut index = self.preferred_index(bucket.hash);
        let mut probe_len = 0usize;

        loop {
            let maybe_val = unsafe { &mut *self.buf.as_ptr().add(index) };
            match maybe_val {
                Some(val) if val.key == bucket.key => {
                    let old = mem::replace(val, bucket);
                    break Some((old.key, old.value));
                }
                Some(val) => {
                    let this_index = self.preferred_index(val.hash);
                    let this_probe_len = self.probe_len(this_index, index);

                    if probe_len > this_probe_len {
                        bucket = mem::replace(val, bucket);
                        probe_len = this_probe_len;
                    }
                }
                None => {
                    *maybe_val = Some(bucket);
                    self.len += 1;
                    break None;
                }
            }
            index = (index + 1) & self.index_mask;
            probe_len += 1;
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        let (ptr, _) = self.get_bucket(key);
        if ptr.is_null() {
            None
        } else {
            unsafe { &*ptr }.as_ref().map(|b| (&b.key, &b.value))
        }
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + fmt::Debug,
    {
        let (ptr, index) = self.get_bucket(key);
        if ptr.is_null() {
            None
        } else {
            let b = unsafe { ptr::replace(ptr, None) }.unwrap();
            self.shift_probe_chain_down(index);
            self.len -= 1;
            Some((b.key, b.value))
        }
    }

    fn get_bucket<Q>(&self, key: &Q) -> (*mut Option<Bucket<K, V>>, usize)
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        if self.is_empty() {
            return (ptr::null_mut(), 0);
        }

        let hash = self.hash_key(key);
        let mut index = self.preferred_index(hash);
        let mut probe_len = 0;

        loop {
            let maybe_val = unsafe { self.buf.as_ptr().add(index) };
            match unsafe { &*maybe_val } {
                Some(b) if b.key.borrow() == key => break (maybe_val, index),
                Some(Bucket { hash, .. }) => {
                    let this_index = self.preferred_index(*hash);
                    let this_probe_len = self.probe_len(this_index, index);

                    if this_probe_len < probe_len {
                        break (ptr::null_mut(), index);
                    }
                }
                None => break (ptr::null_mut(), index),
            }
            index = (index + 1) & self.index_mask;
            probe_len += 1;
        }
    }

    /// This function assumes that the value at `self.buf[start_index]` can be overwritten
    fn shift_probe_chain_down(&mut self, start_index: usize) {
        // Search through the probe chain and move every following item in chain down by one
        let mut index = start_index;
        let mut to_overwrite = unsafe { &mut *self.buf.as_ptr().add(index) };
        *to_overwrite = None;
        loop {
            index = (index + 1) & self.index_mask;
            let next = unsafe { &mut *self.buf.as_ptr().add(index) };
            match next {
                Some(Bucket { key, .. }) => {
                    let preferred_index = self.preferred_index(self.hash_key(&key));
                    if preferred_index != index {
                        *to_overwrite = next.take();
                        to_overwrite = next;
                    } else {
                        // There cannot be more items to shift down, otherwise
                        // this item couldn't be on it's preferred spot
                        break;
                    }
                }
                // at least to_overwrite is None, so we are bound to hit it at some point
                // more realistically since the load_factor is < 1, there are other empty slots as well,
                // above condition could still be hit for very small capacities
                None => break,
            }
        }
    }

    fn hash_key<Q>(&self, key: &Q) -> u64
    where
        Q: Hash,
    {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

impl<K, V> HashMap<K, V> {
    fn grow(&mut self)
    where
        K: Eq + Hash,
    {
        let new_cap = if self.cap == 0 {
            Self::INITIAL_CAP
        } else {
            2 * self.cap
        };

        self.grow_to(new_cap);
    }

    /// # PANICS
    ///
    /// * if `new_cap` is not power of two
    fn grow_to(&mut self, new_cap: usize)
    where
        K: Eq + Hash,
    {
        assert!(new_cap.is_power_of_two());
        if new_cap <= self.cap {
            return;
        }

        // SAFETY: TODO
        let new_buf = unsafe { Self::alloc_new_buf_initialized(new_cap) };
        let (old_buf, old_cap) = unsafe { self.swap_buf(new_buf, new_cap) };

        if old_cap != 0 {
            // drop old buffer
            let old_layout = Self::layout(old_cap);
            unsafe { alloc::dealloc(old_buf.as_ptr().cast::<u8>(), old_layout) }
        }
    }

    /// Allocates new buffer with capacity `new_cap` and initializes all the values to `None`.
    ///
    /// # SAFETY
    ///
    /// * `new_cap > 0`
    ///
    /// # ABORTS
    ///
    /// * if allocation fails
    ///
    /// # PANICS
    ///
    /// * if `new_cap * mem::size_of::<Option<Bucket<K, V>>>() > isize::MAX`
    unsafe fn alloc_new_buf_initialized(new_cap: usize) -> NonNull<Option<Bucket<K, V>>> {
        let new_layout = Self::layout(new_cap);
        let new_buf = unsafe { alloc::alloc(new_layout) };
        if new_buf.is_null() {
            alloc::handle_alloc_error(new_layout);
        } else {
            let new_buf = new_buf.cast::<Option<Bucket<K, V>>>();
            // init to `None`s
            for i in 0..new_cap {
                unsafe { new_buf.add(i).write(None) };
            }

            unsafe { NonNull::new_unchecked(new_buf) }
        }
    }

    /// Swap current buffer with new one by moving all the items from old buffer into new
    ///
    /// # SAFETY
    ///
    /// * `new_buf` must have capacity `new_cap` and all the values must be initialized to `None`
    /// * `new_cap >= self.cap`
    unsafe fn swap_buf(
        &mut self,
        new_buf: NonNull<Option<Bucket<K, V>>>,
        new_cap: usize,
    ) -> (NonNull<Option<Bucket<K, V>>>, usize)
    where
        K: Eq + Hash,
    {
        let old_buf = mem::replace(&mut self.buf, new_buf);
        let old_cap = mem::replace(&mut self.cap, new_cap);
        self.index_mask = self.cap - 1;
        self.len = 0;

        // insert all items into the new buffer
        for i in 0..old_cap {
            let it = unsafe { old_buf.as_ptr().add(i).read() };
            match it {
                Some(b) => {
                    unsafe { self.insert_unchecked(b) };
                }
                None => continue,
            }
        }

        (old_buf, old_cap)
    }
}

#[cfg(test)]
impl<K, V> MapMetrics<K, V> for HashMap<K, V>
where
    K: Hash + Eq,
{
    fn get_with_metrics<Q>(&self, key: &Q) -> Option<(&K, &V, usize)>
    where
        Q: Eq + Hash,
        K: Borrow<Q>,
    {
        if self.is_empty() {
            return None;
        }

        let hash = self.hash_key(key);
        let mut index = self.preferred_index(hash);
        let mut probe_len = 0;

        loop {
            let maybe_val = unsafe { self.buf.as_ptr().add(index) };
            match unsafe { &*maybe_val } {
                Some(b) if b.key.borrow() == key => break Some((&b.key, &b.value, probe_len)),
                Some(Bucket { hash, .. }) => {
                    let this_index = self.preferred_index(*hash);
                    let this_probe_len = self.probe_len(this_index, index);

                    if this_probe_len < probe_len {
                        break None;
                    }
                }
                None => break None,
            }
            index = (index + 1) & self.index_mask;
            probe_len += 1;
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn cap(&self) -> usize {
        self.cap
    }

    fn load_factor(&self) -> f64 {
        self.load_factor()
    }

    fn name(&self) -> &'static str {
        "Robin hood hashing"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert() {
        let mut m = HashMap::<i32, i32>::new();
        assert!(m.is_empty());
        m.insert(1, 11);
        assert_eq!(m.len(), 1);
        m.insert(2, 21);
        m.insert(3, 31);
        m.insert(5, 51);
        assert_eq!(m.len(), 4);
        m.insert(4, 41);

        assert_eq!(m.get(&1), Some((&1, &11)));
        assert_eq!(m.get(&2), Some((&2, &21)));
        assert_eq!(m.get(&3), Some((&3, &31)));
        assert_eq!(m.get(&4), Some((&4, &41)));
        assert_eq!(m.get(&5), Some((&5, &51)));
        assert_eq!(m.get(&6), None);

        assert_eq!(m.insert(4, 42), Some((4, 41)));
        assert_eq!(m.get(&4), Some((&4, &42)));
    }

    #[test]
    fn remove() {
        let mut m = HashMap::new();
        assert_eq!(m.remove(&1), None);

        m.insert(1, 11);
        m.insert(2, 21);
        m.insert(3, 31);
        m.insert(5, 51);
        m.insert(4, 41);

        assert_eq!(m.remove(&2), Some((2, 21)));
        assert_eq!(m.remove(&2), None);

        assert_eq!(m.remove(&1), Some((1, 11)));
        assert_eq!(m.remove(&1), None);

        assert_eq!(m.remove(&3), Some((3, 31)));
        assert_eq!(m.remove(&3), None);

        assert_eq!(m.remove(&4), Some((4, 41)));
        assert_eq!(m.remove(&4), None);

        assert_eq!(m.remove(&5), Some((5, 51)));
        assert_eq!(m.remove(&5), None);
        assert!(m.is_empty())
    }

    #[test]
    fn remove_same_hash() {
        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
        struct SameHash(i32);

        // They all hash to same value, so they must hit the same index in the
        // map and thus are part of same probe chain
        impl Hash for SameHash {
            fn hash<H: Hasher>(&self, state: &mut H) {
                1.hash(state);
            }
        }

        let mut m = HashMap::new();
        assert_eq!(m.remove(&SameHash(1)), None);

        m.insert(SameHash(1), 11);
        m.insert(SameHash(2), 21);
        m.insert(SameHash(3), 31);
        m.insert(SameHash(5), 51);
        m.insert(SameHash(4), 41);

        assert_eq!(m.remove(&SameHash(2)), Some((SameHash(2), 21)));
        assert_eq!(m.remove(&SameHash(1)), Some((SameHash(1), 11)));
        assert_eq!(m.remove(&SameHash(3)), Some((SameHash(3), 31)));
        assert_eq!(m.remove(&SameHash(4)), Some((SameHash(4), 41)));
        assert_eq!(m.remove(&SameHash(5)), Some((SameHash(5), 51)));

        assert!(m.is_empty())
    }

    #[test]
    fn get() {
        let mut m = HashMap::new();
        assert_eq!(m.get(&1), None);

        m.insert(1, 11);
        m.insert(2, 21);
        m.insert(3, 31);
        m.insert(5, 51);
        m.insert(4, 41);

        assert_eq!(m.get(&2), Some((&2, &21)));
        assert_eq!(m.get(&1), Some((&1, &11)));
        assert_eq!(m.get(&3), Some((&3, &31)));
        assert_eq!(m.get(&4), Some((&4, &41)));
        assert_eq!(m.get(&5), Some((&5, &51)));
        assert_eq!(m.get(&6), None);
    }

    #[test]
    fn drop_empty() {
        let m = HashMap::<i32, i32>::new();
        drop(m);
    }

    mod proptests {
        use proptest::prelude::*;
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        use super::*;

        #[cfg(not(miri))]
        const MAP_SIZE: usize = 1000;
        #[cfg(miri)]
        const MAP_SIZE: usize = 50;

        #[cfg(not(miri))]
        const PROPTEST_CASES: u32 = 1000;
        #[cfg(miri)]
        const PROPTEST_CASES: u32 = 10;

        proptest!(
            #![proptest_config(ProptestConfig::with_cases(PROPTEST_CASES))]
            #[test]
            fn insert_get(
                mut inserts in proptest::collection::vec(0..10000i32, 0..MAP_SIZE),
                access in proptest::collection::vec(0..10000i32, 0..10)
            ) {
                let ref_hmap = std::collections::HashMap::<i32, i32, RandomState>::from_iter(inserts.iter().map(|v| (*v, *v)));
                let mut hmap = HashMap::with_capacity(ref_hmap.len());
                for v in &inserts {
                    hmap.insert(*v, *v);
                }

                assert_eq!(ref_hmap.len(), hmap.len());

                inserts.shuffle(&mut thread_rng());
                for key in inserts.iter().chain(access.iter()) {
                    assert_eq!(ref_hmap.get_key_value(key), hmap.get(key));
                }
            }

            #[test]
            fn remove(
                mut inserts in proptest::collection::vec(0..10000i32, 0..MAP_SIZE),
                access in proptest::collection::vec(0..10000i32, 0..10)
            ) {
                let mut ref_hmap = std::collections::HashMap::<i32, i32, RandomState>::from_iter(inserts.iter().map(|v| (*v, *v)));
                let mut hmap = HashMap::with_capacity(ref_hmap.len());
                for v in &inserts {
                    hmap.insert(*v, *v);
                }

                assert_eq!(ref_hmap.len(), hmap.len());

                inserts.shuffle(&mut thread_rng());
                for key in access.iter().chain(inserts.iter()) {
                    assert_eq!(ref_hmap.remove_entry(key), hmap.remove(key));
                }
            }
        );
    }
}
