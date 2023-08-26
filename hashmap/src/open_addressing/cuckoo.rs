//! Hash map with cuckoo hashing

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

pub struct HashMap<K, V> {
    buf1: NonNull<Option<(K, V)>>,
    buf2: NonNull<Option<(K, V)>>,
    /// Capacity of one buffer, total map capacity is 2*cap
    cap: usize,
    index_mask: usize,
    len: usize,
    hash_builder1: RandomState,
    hash_builder2: RandomState,
    crit_load_factor: f64,
    marker: PhantomData<(K, V)>,
}

impl<K, V> Drop for HashMap<K, V> {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }

        for i in 0..self.cap {
            let it = unsafe { self.buf1.as_ptr().add(i) };
            unsafe { ptr::drop_in_place(it) };
        }

        for i in 0..self.cap {
            let it = unsafe { self.buf2.as_ptr().add(i) };
            unsafe { ptr::drop_in_place(it) };
        }

        let layout = Self::layout(self.cap);
        unsafe { alloc::dealloc(self.buf1.as_ptr().cast::<u8>(), layout) }
        unsafe { alloc::dealloc(self.buf2.as_ptr().cast::<u8>(), layout) }
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
            buf1: NonNull::dangling(),
            buf2: NonNull::dangling(),
            cap: 0,
            index_mask: 0,
            len: 0,
            crit_load_factor: self.crit_load_factor,
            hash_builder1: self.hash_builder1.clone(),
            hash_builder2: self.hash_builder2.clone(),
            marker: self.marker,
        };
        s.grow_to(self.cap);
        for i in 0..self.cap {
            let it = unsafe { &*self.buf1.as_ptr().add(i) };
            if let Some((k, v)) = it {
                s.insert(k.clone(), v.clone());
            }
        }

        for i in 0..self.cap {
            let it = unsafe { &*self.buf2.as_ptr().add(i) };
            if let Some((k, v)) = it {
                s.insert(k.clone(), v.clone());
            }
        }

        s
    }
}

impl<K, V> fmt::Debug for HashMap<K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HashMap")
            .field(
                "buf1",
                &DebugHashMapBuf {
                    buf: self.buf1,
                    cap: self.cap,
                    marker: PhantomData,
                },
            )
            .field(
                "buf2",
                &DebugHashMapBuf {
                    buf: self.buf2,
                    cap: self.cap,
                    marker: PhantomData,
                },
            )
            .field("cap", &self.cap)
            .field("len", &self.len)
            .field("hash_builder1", &self.hash_builder1)
            .field("hash_builder2", &self.hash_builder2)
            .finish()
    }
}

struct DebugHashMapBuf<'a, K, V> {
    buf: NonNull<Option<(K, V)>>,
    cap: usize,
    marker: PhantomData<&'a Option<(K, V)>>,
}

impl<'a, K, V> fmt::Debug for DebugHashMapBuf<'a, K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();

        for i in 0..self.cap {
            let it = unsafe { &*self.buf.as_ptr().add(i) };
            list.entry(it);
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
        let (buf1, buf2, cap, index_mask) = if capacity > 0 {
            let capacity = (capacity as f64 / lf / 2.0 + 1.0) as usize;
            let capacity = round_up_to_power_of_two(capacity);
            debug_assert!(capacity.is_power_of_two());
            debug_assert!(capacity > 0);
            let buf1 = unsafe { Self::alloc_new_buf_initialized(capacity) };
            let buf2 = unsafe { Self::alloc_new_buf_initialized(capacity) };
            (buf1, buf2, capacity, capacity - 1)
        } else {
            (NonNull::dangling(), NonNull::dangling(), 0, 0)
        };
        Self {
            buf1,
            buf2,
            cap,
            index_mask,
            len: 0,
            hash_builder1: RandomState::new(),
            hash_builder2: RandomState::new(),
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

    pub fn capacity(&self) -> usize {
        self.cap * 2
    }

    fn preferred_index(&self, hash: u64) -> usize {
        debug_assert!(self.cap < isize::MAX as usize);
        debug_assert!(self.cap.is_power_of_two());
        // SAFETY: cap <= isize::MAX, hence the result after modulo must be < isize::MAX
        (hash & self.index_mask as u64) as usize
    }

    fn load_factor(&self) -> f64 {
        if self.cap == 0 {
            return f64::INFINITY;
        }

        self.len as f64 / (self.capacity() as f64)
    }

    fn layout(cap: usize) -> Layout {
        Layout::array::<Option<(K, V)>>(cap).unwrap()
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

        debug_assert!(self.len < self.cap * 2);
        unsafe { self.insert_unchecked(key, value) }
    }

    /// # SAFETY
    ///
    /// * Self must have the capacity for 1 more item
    ///   (ideally we would also not exceed `load_factor > Self::CRIT_LOAD_FACTOR`
    ///   but that's not a safety requirement)
    unsafe fn insert_unchecked(&mut self, mut key: K, mut value: V) -> Option<(K, V)> {
        // We need to check both buffers to see if key already exists.
        // Start with buf2 so that buf1 would be the first one we try to insert new items.
        let hash = self.hash_key2(&key);
        let index = self.preferred_index(hash);
        let maybe_val = unsafe { &mut *self.buf2.as_ptr().add(index) };
        match maybe_val {
            Some(val) if val.0 == key => {
                let old = mem::replace(val, (key, value));
                return Some(old);
            }
            _ => {}
        }

        let mut i = 0;
        loop {
            let hash = self.hash_key1(&key);
            let index = self.preferred_index(hash);
            let maybe_val = unsafe { &mut *self.buf1.as_ptr().add(index) };
            match maybe_val {
                Some(val) if val.0 == key => {
                    let old = mem::replace(val, (key, value));
                    break Some(old);
                }
                Some(val) => {
                    (key, value) = mem::replace(val, (key, value));
                }
                None => {
                    *maybe_val = Some((key, value));
                    self.len += 1;
                    break None;
                }
            }

            let hash = self.hash_key2(&key);
            let index = self.preferred_index(hash);
            let maybe_val = unsafe { &mut *self.buf2.as_ptr().add(index) };
            match maybe_val {
                Some(val) if val.0 == key => {
                    let old = mem::replace(val, (key, value));
                    break Some(old);
                }
                Some(val) => {
                    (key, value) = mem::replace(val, (key, value));
                }
                None => {
                    *maybe_val = Some((key, value));
                    self.len += 1;
                    break None;
                }
            }
            i += 1;

            if i == self.cap {
                self.grow();
                i = 0;
            }
        }
    }

    pub fn get<Q>(&mut self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        match self.get_bucket(key) {
            Some(b) => match unsafe { &*b } {
                Some((k, v)) => Some((k, v)),
                _ => unreachable!(),
            },
            None => None,
        }
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + fmt::Debug,
    {
        match self.get_bucket(key) {
            Some(b) => {
                let b = unsafe { ptr::replace(b, None) };
                self.len -= 1;
                match b {
                    Some((k, v)) => Some((k, v)),
                    _ => unreachable!(),
                }
            }
            None => None,
        }
    }

    fn get_bucket<Q>(&mut self, key: &Q) -> Option<*mut Option<(K, V)>>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        if self.is_empty() {
            return None;
        }

        let hash = self.hash_key1(key);
        let index = self.preferred_index(hash);
        let maybe_val = unsafe { self.buf1.as_ptr().add(index) };
        match unsafe { &*maybe_val } {
            Some((ref k, _)) if k.borrow() == key => return Some(maybe_val),
            _ => {}
        }

        let hash = self.hash_key2(key);
        let index = self.preferred_index(hash);
        let maybe_val = unsafe { self.buf2.as_ptr().add(index) };
        match unsafe { &*maybe_val } {
            Some((ref k, _)) if k.borrow() == key => Some(maybe_val),
            _ => None,
        }
    }

    fn hash_key<Q>(&self, key: &Q) -> (u64, u64)
    where
        Q: Hash,
    {
        let mut hasher = self.hash_builder1.build_hasher();
        key.hash(&mut hasher);
        let h1 = hasher.finish();

        let mut hasher = self.hash_builder1.build_hasher();
        key.hash(&mut hasher);
        let h2 = hasher.finish();
        (h1, h2)
    }

    fn hash_key1<Q>(&self, key: &Q) -> u64
    where
        Q: Hash,
    {
        let mut hasher = self.hash_builder1.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }

    fn hash_key2<Q>(&self, key: &Q) -> u64
    where
        Q: Hash,
    {
        let mut hasher = self.hash_builder2.build_hasher();
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
        let new_buf1 = unsafe { Self::alloc_new_buf_initialized(new_cap) };
        let new_buf2 = unsafe { Self::alloc_new_buf_initialized(new_cap) };
        let (old_buf1, old_buf2, old_cap) = unsafe { self.swap_buf(new_buf1, new_buf2, new_cap) };

        if old_cap != 0 {
            // drop old buffer
            let old_layout = Self::layout(old_cap);
            unsafe { alloc::dealloc(old_buf1.as_ptr().cast::<u8>(), old_layout) }
            unsafe { alloc::dealloc(old_buf2.as_ptr().cast::<u8>(), old_layout) }
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
    unsafe fn alloc_new_buf_initialized(new_cap: usize) -> NonNull<Option<(K, V)>> {
        let new_layout = Self::layout(new_cap);
        let new_buf = unsafe { alloc::alloc(new_layout) };
        if new_buf.is_null() {
            alloc::handle_alloc_error(new_layout);
        } else {
            let new_buf = new_buf.cast::<Option<(K, V)>>();
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
        new_buf1: NonNull<Option<(K, V)>>,
        new_buf2: NonNull<Option<(K, V)>>,
        new_cap: usize,
    ) -> (NonNull<Option<(K, V)>>, NonNull<Option<(K, V)>>, usize)
    where
        K: Eq + Hash,
    {
        let old_buf1 = mem::replace(&mut self.buf1, new_buf1);
        let old_buf2 = mem::replace(&mut self.buf2, new_buf2);
        let old_cap = mem::replace(&mut self.cap, new_cap);
        self.index_mask = self.cap - 1;
        self.len = 0;

        // insert all items into the new buffers
        for i in 0..old_cap {
            let it = unsafe { old_buf1.as_ptr().add(i).read() };
            match it {
                Some((k, v)) => {
                    unsafe { self.insert_unchecked(k, v) };
                }
                _ => continue,
            }
        }

        for i in 0..old_cap {
            let it = unsafe { old_buf2.as_ptr().add(i).read() };
            match it {
                Some((k, v)) => {
                    unsafe { self.insert_unchecked(k, v) };
                }
                _ => continue,
            }
        }

        (old_buf1, old_buf2, old_cap)
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

        let hash = self.hash_key1(key);
        let index = self.preferred_index(hash);
        let maybe_val = unsafe { self.buf1.as_ptr().add(index) };
        match unsafe { &*maybe_val } {
            Some((ref k, v)) if k.borrow() == key => return Some((k, v, 0)),
            _ => {}
        }

        let hash = self.hash_key2(key);
        let index = self.preferred_index(hash);
        let maybe_val = unsafe { self.buf2.as_ptr().add(index) };
        match unsafe { &*maybe_val } {
            Some((ref k, v)) if k.borrow() == key => Some((k, v, 1)),
            _ => None,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn cap(&self) -> usize {
        self.capacity()
    }

    fn load_factor(&self) -> f64 {
        self.load_factor()
    }

    fn name(&self) -> &'static str {
        "Cuckoo hashing"
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
    #[ignore = "broken, don't know right know how to fix"]
    fn remove_same_hash() {
        // The issue here is that is all values hash to same hash then we
        // always hit the same two buckets in both buffers and thus end up
        // in infinite loop.

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
        println!("{m:#?}");
        m.insert(SameHash(2), 21);
        println!("{m:#?}");
        m.insert(SameHash(3), 31);
        println!("{m:#?}");
        m.insert(SameHash(5), 51);
        println!("{m:#?}");
        m.insert(SameHash(4), 41);
        println!("{m:#?}");

        assert_eq!(m.remove(&SameHash(2)), Some((SameHash(2), 21)));
        assert_eq!(m.remove(&SameHash(1)), Some((SameHash(1), 11)));
        assert_eq!(m.remove(&SameHash(3)), Some((SameHash(3), 31)));
        assert_eq!(m.remove(&SameHash(4)), Some((SameHash(4), 41)));
        assert_eq!(m.remove(&SameHash(5)), Some((SameHash(5), 51)));

        assert!(m.is_empty());
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
        m.insert(6, 41);
        m.insert(7, 41);
        m.insert(8, 41);
        m.insert(9, 41);

        assert_eq!(m.get(&2), Some((&2, &21)));
        assert_eq!(m.get(&1), Some((&1, &11)));
        assert_eq!(m.get(&3), Some((&3, &31)));
        assert_eq!(m.get(&4), Some((&4, &41)));
        assert_eq!(m.get(&5), Some((&5, &51)));
        assert_eq!(m.get(&6), Some((&6, &41)));
        assert_eq!(m.get(&7), Some((&7, &41)));
        assert_eq!(m.get(&8), Some((&8, &41)));
        assert_eq!(m.get(&9), Some((&9, &41)));
        assert_eq!(m.get(&10), None);
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

                assert_eq!(ref_hmap.len(), hmap.len(), "wrong len");

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

            #[test]
            #[cfg_attr(miri, ignore = "nothing for miri to really check, no need to waste time")]
            fn with_cap(cap in 0..100_000usize, lf in 0.5..0.999) {
                let map = HashMap::<u8, ()>::with_capacity_and_load_factor(cap, lf);
                let will_be_lf = cap as f64/map.capacity() as f64;
                assert!(will_be_lf < lf);
                assert!(map.cap.is_power_of_two());
            }
        );
    }
}
