//! Hash map with chaining vecs

use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash, Hasher};
use core::marker::PhantomData;
use core::mem;
use std::collections::hash_map::RandomState;

type Chain<K, V> = Vec<(K, V)>;

#[derive(Debug)]
pub struct HashMap<K, V> {
    buf: Vec<Chain<K, V>>,
    cap: usize,
    len: usize,
    hash_builder: RandomState,
    marker: PhantomData<Chain<K, V>>,
}

impl<K, V> HashMap<K, V>
where
    K: Hash,
{
    const CRIT_LOAD_FACTOR: f64 = 2.0;
    const INITIAL_CAP: usize = 4;

    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
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

        let hash = self.hash_key(&key);
        let index = self.get_index(hash);
        let chain = &mut self.buf[index];
        let pair = (key, value);
        match chain.iter_mut().find(|(k, _)| k == &pair.0) {
            Some(existing) => {
                let old = mem::replace(existing, pair);
                Some(old)
            }
            None => {
                chain.push(pair);
                self.len += 1;
                None
            }
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
        let index = self.get_index(hash);
        let chain = &self.buf[index];
        chain
            .iter()
            .find(|(k, _)| k.borrow() == key)
            .map(|(k, v)| (k, v))
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        if self.is_empty() {
            return None;
        }

        let hash = self.hash_key(key);
        let index = self.get_index(hash);
        let chain = &mut self.buf[index];

        let pos = chain.iter().position(|(k, _)| k.borrow() == key);
        pos.map(|pos| {
            self.len -= 1;
            chain.swap_remove(pos)
        })
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

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            Self::INITIAL_CAP
        } else {
            2 * self.cap
        };

        let mut new_buf = Vec::new();
        new_buf.reserve_exact(new_cap);

        self.cap = new_cap;
        assert!(self.cap <= new_buf.capacity());

        for _ in 0..self.cap {
            new_buf.push(Vec::new());
        }

        let old_buf = mem::replace(&mut self.buf, new_buf);
        self.extend_non_existing(old_buf.into_iter().flatten());
    }

    /// Extend `self` with the `items` without checking if the key already exists.
    /// This is this method assumes that none of the key already exist in the map.
    fn extend_non_existing(&mut self, items: impl Iterator<Item = (K, V)>) {
        for (k, v) in items {
            let hash = self.hash_key(&k);
            let index = self.get_index(hash);
            self.buf[index].push((k, v));
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
        println!("{m:?}");

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
