//! Hash map with Robin Hood hashing variant of linear probing

extern crate alloc as crate_alloc;

use core::alloc::Layout;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash, Hasher};
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::{fmt, mem};
use std::collections::hash_map::RandomState;

use crate_alloc::alloc;

struct Bucket<K, V> {
    key: K,
    value: V,
    hash: u64,
}

pub struct HashMap<K, V> {
    buf: NonNull<Option<Bucket<K, V>>>,
    cap: usize,
    len: usize,
    hash_builder: RandomState,
    marker: PhantomData<(K, V)>,
}

impl<K, V> fmt::Debug for HashMap<K, V>
where
    K: fmt::Debug + Hash,
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
    K: fmt::Debug + Hash,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn hash_key<Q>(hash_builder: &RandomState, key: &Q) -> u64
        where
            Q: Hash,
        {
            let mut hasher = hash_builder.build_hasher();
            key.hash(&mut hasher);
            hasher.finish()
        }

        let mut list = f.debug_list();

        for i in 0..self.map.cap {
            let it = unsafe { &*self.map.buf.as_ptr().add(i) };
            let it = it.as_ref().map(|b| {
                let hash = self.map.hash_key(&b.key);
                let orig_index = self.map.get_index(hash);
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

impl<K, V> HashMap<K, V>
where
    K: Hash + fmt::Debug,
    V: fmt::Debug,
{
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

    pub fn insert(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        K: Eq,
    {
        if self.load_factor() > Self::CRIT_LOAD_FACTOR {
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
    unsafe fn insert_unchecked(&mut self, mut bucket: Bucket<K, V>) -> Option<(K, V)>
    where
        K: Eq,
    {
        let mut index = self.get_index(bucket.hash);
        let mut probe_len = 0usize;

        loop {
            let maybe_val = unsafe { &mut *self.buf.as_ptr().add(index) };
            match maybe_val {
                Some(val) if val.key == bucket.key => {
                    let old = mem::replace(val, bucket);
                    break Some((old.key, old.value));
                }
                Some(val) => {
                    let this_index = self.get_index(val.hash);
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
            index = (index + 1) % self.cap;
            probe_len += 1;
        }
    }

    fn probe_len(&self, orig_index: usize, actual_index: usize) -> usize {
        if actual_index < orig_index {
            // probe must wrap around
            (self.cap - orig_index) + actual_index
        } else {
            actual_index - orig_index
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

        loop {
            let maybe_val = unsafe { &*self.buf.as_ptr().add(index) };
            match maybe_val {
                Some(b) if b.key.borrow() == key => {
                    break Some((&b.key, &b.value));
                }
                Some(_) => {}
                None => {
                    break None;
                }
            }
            index = (index + 1) % self.cap;
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
        loop {
            let maybe_val = unsafe { &mut *self.buf.as_ptr().add(index) };
            match maybe_val.take() {
                Some(b) if b.key.borrow() == key => {
                    self.shift_probe_chain_down(index);
                    self.len -= 1;
                    break Some((b.key, b.value));
                }
                val @ Some(_) => {
                    *maybe_val = val;
                }
                None => {
                    break None;
                }
            }
            index = (index + 1) % self.cap;
        }
    }

    /// This function assumes that the value at `self.buf[start_index]` can be overwritten
    fn shift_probe_chain_down(&mut self, start_index: usize) {
        // Search through the probe chain and move every following item in chain down by one
        let mut index = start_index;
        let mut to_overwrite = unsafe { &mut *self.buf.as_ptr().add(index) };
        *to_overwrite = None;
        loop {
            index = (index + 1) % self.cap;
            let next = unsafe { &mut *self.buf.as_ptr().add(index) };
            match next {
                Some(Bucket { key, .. }) => {
                    let preferred_index = self.get_index(self.hash_key(&key));
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

    fn get_index(&self, hash: u64) -> usize {
        debug_assert!(self.cap < isize::MAX as usize);
        // SAFETY: cap <= isize::MAX, hence the result after modulo must be < isize::MAX
        (hash % self.cap as u64) as usize
    }

    fn hash_key<Q>(&self, key: &Q) -> u64
    where
        Q: Hash,
    {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
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

    fn grow(&mut self)
    where
        K: Eq,
    {
        let new_cap = if self.cap == 0 {
            Self::INITIAL_CAP
        } else {
            2 * self.cap
        };

        self.grow_to(new_cap);
    }

    fn grow_to(&mut self, new_cap: usize)
    where
        K: Eq,
    {
        if new_cap <= self.cap {
            return;
        }

        let old_layout = Self::layout(self.cap);
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

            let new_buf = unsafe { NonNull::new_unchecked(new_buf) };
            let old_buf = mem::replace(&mut self.buf, new_buf).as_ptr();
            let old_cap = mem::replace(&mut self.cap, new_cap);
            self.len = 0;

            // insert all items into the new buffer
            for i in 0..old_cap {
                let it = unsafe { old_buf.add(i).read() };
                match it {
                    Some(b) => {
                        unsafe { self.insert_unchecked(b) };
                    }
                    None => continue,
                }
            }

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
}
