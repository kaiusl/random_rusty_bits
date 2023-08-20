//! Hash map with linear probing and lazy deletion

extern crate alloc as crate_alloc;

use core::alloc::Layout;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash, Hasher};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use core::{fmt, mem};
use std::collections::hash_map::RandomState;

use crate_alloc::alloc;

pub struct HashMap<K, V> {
    buf: NonNull<Bucket<K, V>>,
    cap: usize,
    len: usize,
    hash_builder: RandomState,
    marker: PhantomData<(K, V)>,
}

#[derive(Debug, Clone)]
enum Bucket<K, V> {
    Occupied((K, V)),
    Empty,
    Deleted,
}

impl<K, V> Drop for HashMap<K, V> {
    fn drop(&mut self) {
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
            len: 0,
            hash_builder: self.hash_builder.clone(),
            marker: self.marker,
        };
        s.grow_to(self.cap);
        for i in 0..self.cap {
            let it = unsafe { &*self.buf.as_ptr().add(i) };
            if let Bucket::Occupied((k, v)) = it {
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
                "buf",
                &DebugHashMapBuf {
                    buf: self.buf,
                    cap: self.cap,
                    marker: PhantomData,
                },
            )
            .field("cap", &self.cap)
            .field("len", &self.len)
            .field("hash_builder", &self.hash_builder)
            .finish()
    }
}

struct DebugHashMapBuf<'a, K, V> {
    buf: NonNull<Bucket<K, V>>,
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
    const CRIT_LOAD_FACTOR: f64 = 0.7;
    const INITIAL_CAP: usize = 4;

    pub fn new() -> Self {
        Self {
            buf: NonNull::dangling(),
            cap: 0,
            len: 0,
            hash_builder: RandomState::new(),
            marker: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn mask(&self) -> usize {
        self.cap - 1
    }

    fn get_index(&self, hash: u64) -> usize {
        debug_assert!(self.cap < isize::MAX as usize);
        debug_assert!(self.cap.is_power_of_two());
        // SAFETY: cap <= isize::MAX, hence the result after modulo must be < isize::MAX
        (hash & self.mask() as u64) as usize
    }

    fn load_factor(&self) -> f64 {
        if self.cap == 0 {
            return f64::INFINITY;
        }

        self.len as f64 / self.cap as f64
    }

    fn layout(cap: usize) -> Layout {
        Layout::array::<Bucket<K, V>>(cap).unwrap()
    }
}

impl<K, V> HashMap<K, V>
where
    K: Hash + Eq,
{
    pub fn insert(&mut self, key: K, value: V) -> Option<(K, V)> {
        if self.load_factor() > Self::CRIT_LOAD_FACTOR {
            self.grow()
        }

        debug_assert!(self.len < self.cap);
        unsafe { self.insert_unchecked(key, value) }
    }

    /// # SAFETY
    ///
    /// * Self must have the capacity for 1 more item
    ///   (ideally we would also not exceed `load_factor > Self::CRIT_LOAD_FACTOR`
    ///   but that's not a safety requirement)
    unsafe fn insert_unchecked(&mut self, key: K, value: V) -> Option<(K, V)> {
        let hash = self.hash_key(&key);
        let mut index = self.get_index(hash);
        let mask = self.mask();

        loop {
            let maybe_val = unsafe { &mut *self.buf.as_ptr().add(index) };
            match maybe_val {
                Bucket::Occupied(val) if val.0 == key => {
                    let old = mem::replace(val, (key, value));
                    break Some(old);
                }
                Bucket::Occupied(_) => {}
                Bucket::Empty | Bucket::Deleted => {
                    *maybe_val = Bucket::Occupied((key, value));
                    self.len += 1;
                    break None;
                }
            }
            index = (index + 1) & mask;
        }
    }

    pub fn get<Q>(&mut self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        if self.is_empty() {
            return None;
        }

        let hash = self.hash_key(key);
        let mut index = self.get_index(hash);
        let mask = self.mask();

        loop {
            let maybe_val = unsafe { &*self.buf.as_ptr().add(index) };
            match maybe_val {
                Bucket::Occupied((ref k, ref v)) if k.borrow() == key => {
                    break Some((k, v));
                }

                Bucket::Occupied(_) | Bucket::Deleted => {}
                Bucket::Empty => {
                    break None;
                }
            }
            index = (index + 1) & mask;
        }
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + fmt::Debug,
    {
        if self.is_empty() {
            return None;
        }

        let hash = self.hash_key(key);
        let mut index = self.get_index(hash);
        let mask = self.mask();
        loop {
            let maybe_val = unsafe { &mut *self.buf.as_ptr().add(index) };
            match maybe_val {
                Bucket::Occupied((ref k, _)) if k.borrow() == key => {
                    let old = mem::replace(maybe_val, Bucket::Deleted);
                    self.len -= 1;
                    match old {
                        Bucket::Occupied(old) => break Some(old),
                        _ => unreachable!(),
                    }
                }
                Bucket::Occupied(_) | Bucket::Deleted => {}
                Bucket::Empty => {
                    break None;
                }
            }
            index = (index + 1) & mask;
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

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            Self::INITIAL_CAP
        } else {
            2 * self.cap
        };

        self.grow_to(new_cap);
    }

    fn grow_to(&mut self, new_cap: usize) {
        if new_cap <= self.cap {
            return;
        }

        let old_layout = Self::layout(self.cap);
        let new_layout = Self::layout(new_cap);

        let new_buf = unsafe { alloc::alloc(new_layout) };

        if new_buf.is_null() {
            alloc::handle_alloc_error(new_layout);
        } else {
            let new_buf = new_buf.cast::<Bucket<K, V>>();
            // init to `None`s
            for i in 0..new_cap {
                unsafe { new_buf.add(i).write(Bucket::Empty) };
            }

            let new_buf = unsafe { NonNull::new_unchecked(new_buf) };
            let old_buf = mem::replace(&mut self.buf, new_buf).as_ptr();
            let old_cap = mem::replace(&mut self.cap, new_cap);
            self.len = 0;

            // insert all items into the new buffer
            for i in 0..old_cap {
                let it = unsafe { old_buf.add(i).read() };
                match it {
                    Bucket::Occupied((k, v)) => {
                        unsafe { self.insert_unchecked(k, v) };
                    }
                    _ => continue,
                }
            }

            if old_cap != 0 {
                // drop old buffer
                unsafe { alloc::dealloc(old_buf.cast::<u8>(), old_layout) }
            }
        }
    }

    fn grow_empty_to(&mut self, new_cap: usize) {
        assert!(self.len == 0);
        if new_cap <= self.cap {
            return;
        }

        let old_layout = Self::layout(self.cap);
        let new_layout = Self::layout(new_cap);

        let new_buf = unsafe { alloc::alloc(new_layout) };

        if new_buf.is_null() {
            alloc::handle_alloc_error(new_layout);
        } else {
            let new_buf = new_buf.cast::<Bucket<K, V>>();
            // init to `None`s
            for i in 0..new_cap {
                unsafe { new_buf.add(i).write(Bucket::Empty) };
            }

            let new_buf = unsafe { NonNull::new_unchecked(new_buf) };
            let old_buf = mem::replace(&mut self.buf, new_buf).as_ptr();
            let old_cap = mem::replace(&mut self.cap, new_cap);

            if old_cap != 0 {
                // drop old buffer
                unsafe { alloc::dealloc(old_buf.cast::<u8>(), old_layout) }
            }
        }
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

        assert_eq!(m.get(&2), Some((&2, &21)));
        assert_eq!(m.get(&1), Some((&1, &11)));
        assert_eq!(m.get(&3), Some((&3, &31)));
        assert_eq!(m.get(&4), Some((&4, &41)));
        assert_eq!(m.get(&5), Some((&5, &51)));
        assert_eq!(m.get(&6), None);
    }
}
