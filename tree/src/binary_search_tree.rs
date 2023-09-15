use core::fmt;
use std::borrow::Borrow;
use std::marker::PhantomData;
use std::ptr::{self, NonNull};

struct Node<K, V> {
    key: K,
    value: V,
    parent: Option<NonNull<Node<K, V>>>,
    left: Option<NonNull<Node<K, V>>>,
    right: Option<NonNull<Node<K, V>>>,
}

impl<K, V> fmt::Debug for Node<K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Node");
        f.field("key", &self.key).field("value", &self.value);

        let mut dbg_opt_node = |name: &str, node: &Option<NonNull<Node<K, V>>>| match node {
            Some(node) => {
                let node = unsafe { node.as_ref() };
                f.field(name, &(&node.key, &node.value));
            }
            None => {
                f.field(name, &None::<K>);
            }
        };

        dbg_opt_node("parent", &self.parent);
        dbg_opt_node("left", &self.left);
        dbg_opt_node("right", &self.right);

        f.finish()
    }
}

/// A binary search tree based map.
///
/// For simplicity we don't allow duplicate keys.
struct BinarySearchTree<K, V> {
    // INVARIANTS:
    //  * if `len > 0` then root is valid pointer to `Node`
    root: NonNull<Node<K, V>>,
    len: usize,
    marker: PhantomData<Box<Node<K, V>>>,
}

impl<K, V> Drop for BinarySearchTree<K, V> {
    fn drop(&mut self) {
        if self.is_empty() {
            return;
        }

        // TODO: handle panics in `K::drop` or `V::drop`

        unsafe fn inner<K, V>(node: NonNull<Node<K, V>>) {
            if let Some(l) = unsafe { (*node.as_ptr()).left } {
                unsafe { inner(l) };
            }
            if let Some(r) = unsafe { (*node.as_ptr()).right } {
                unsafe { inner(r) };
            }
            let _ = unsafe { Box::from_raw(node.as_ptr()) };
        }

        self.len = 0;
        unsafe { inner(self.root) }
    }
}

impl<K, V> fmt::Debug for BinarySearchTree<K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct TreeDebug<'a, K, V> {
            root: NonNull<Node<K, V>>,
            marker: PhantomData<&'a Node<K, V>>,
        }

        impl<K, V> fmt::Debug for TreeDebug<'_, K, V>
        where
            K: fmt::Debug,
            V: fmt::Debug,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut f = f.debug_list();

                let mut func = |node: NonNull<Node<K, V>>| {
                    let node = unsafe { node.as_ref() };
                    f.entry(&node);
                };

                unsafe { BinarySearchTree::inorder_for_each_core(self.root, &mut func) };

                f.finish()
            }
        }

        let mut f = f.debug_struct("BinarySearchTree");
        f.field("len", &self.len);

        match self.len {
            0 => {
                f.field("root", &None::<K>);
                let nodes: &[K] = &[];
                f.field("nodes", &nodes);
            }
            _ => {
                f.field("root", &Some(unsafe { self.root.as_ref() }));
                f.field(
                    "nodes",
                    &TreeDebug {
                        root: self.root,
                        marker: PhantomData,
                    },
                );
            }
        }

        f.finish()
    }
}

impl<K, V> BinarySearchTree<K, V> {
    pub fn new() -> Self {
        Self {
            root: NonNull::dangling(),
            len: 0,
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn inorder_for_each<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &mut V),
    {
        if self.is_empty() {
            return;
        }

        let mut f = |mut node: NonNull<Node<K, V>>| {
            let node = unsafe { node.as_mut() };
            f(&node.key, &mut node.value)
        };
        unsafe { Self::inorder_for_each_core(self.root, &mut f) }
    }

    unsafe fn inorder_for_each_core<F>(node: NonNull<Node<K, V>>, f: &mut F)
    where
        F: FnMut(NonNull<Node<K, V>>),
    {
        if let Some(l) = unsafe { (*node.as_ptr()).left } {
            unsafe { Self::inorder_for_each_core(l, f) };
        }
        f(node);
        if let Some(r) = unsafe { (*node.as_ptr()).right } {
            unsafe { Self::inorder_for_each_core(r, f) };
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.get_raw(key)
            .map(|node| unsafe { self.node_as_refs(node) })
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<(&K, &mut V)>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.get_raw(key)
            .map(|node| unsafe { self.node_as_muts(node) })
    }

    fn get_raw<Q>(&self, key: &Q) -> Option<NonNull<Node<K, V>>>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        if self.is_empty() {
            return None;
        }

        let mut x = self.root;
        loop {
            match key.cmp(unsafe { (*x.as_ptr()).key.borrow() }) {
                std::cmp::Ordering::Less => match unsafe { &(*x.as_ptr()).left } {
                    Some(left) => {
                        x = *left;
                    }
                    None => break,
                },
                std::cmp::Ordering::Equal => return Some(x),
                std::cmp::Ordering::Greater => match unsafe { &(*x.as_ptr()).right } {
                    Some(right) => {
                        x = *right;
                    }
                    None => break,
                },
            }
        }

        None
    }

    pub fn min(&self) -> Option<(&K, &V)> {
        if self.is_empty() {
            return None;
        }
        let min = unsafe { self.min_of(self.root) };
        unsafe { Some(self.node_as_refs(min)) }
    }

    unsafe fn min_of(&self, root: NonNull<Node<K, V>>) -> NonNull<Node<K, V>> {
        let mut x = root;
        while let Some(left) = unsafe { (*x.as_ptr()).left } {
            x = left;
        }

        x
    }

    pub fn max(&self) -> Option<(&K, &V)> {
        if self.is_empty() {
            return None;
        }
        let max = unsafe { self.max_of(self.root) };
        unsafe { Some(self.node_as_refs(max)) }
    }

    unsafe fn max_of(&self, root: NonNull<Node<K, V>>) -> NonNull<Node<K, V>> {
        let mut x = root;
        while let Some(right) = unsafe { (*x.as_ptr()).right } {
            x = right;
        }

        x
    }

    pub fn successor<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q> + Eq,
        Q: Ord,
    {
        match self.get_raw(key) {
            Some(node) => unsafe {
                self.successor_core(node)
                    .map(|node| self.node_as_refs(node))
            },
            None => None,
        }
    }

    unsafe fn successor_core(&self, node: NonNull<Node<K, V>>) -> Option<NonNull<Node<K, V>>>
    where
        K: Eq,
    {
        //       ┌────────── 34 ─────────┐
        //       │                       │
        // ┌──── 2 ────┐                 58 ────┐
        // │           │                        │
        // 1      ┌─── 9 ────┐              ┌── 77 ──┐
        //        │          │              │        │
        //     ┌─ 6       ┌─ 20 ─┐      ┌─ 71 ─┐     82
        //     │          │      │      │      │
        //     5         12 ─┐   24    67      75
        //                   │
        //                   13

        let mut node = node.as_ptr();
        match unsafe { (*node).right } {
            // 9 -> 12, 2 -> 5, 58 -> 67 ...
            //
            // A right subtree contains items that are larger than node but smaller than any other larger than node item in the tree --
            // that is smaller than the successor of node if it didn't have a right subtree.
            // Thus if a right subtree exists the successor must be in it.
            //
            // Another way to think about it would be to consider that a right subtree only exists if we insert > node item
            // after the node. When we move down the tree and the new item is larger than any other item before the node,
            // it would end up in their right subtree whereas the node is in it's left subtree. Thus the item that ends up
            // it the node's right subtree must be larger than node but smaller than any other item that's larger than the node.
            Some(right) => unsafe { Some(self.min_of(right)) },
            _ => {
                // 6 -> 9, 1 -> 2, 13 -> 20, 24 -> 34 ...
                // Move up the parents and find the first node which is the left child of it's parent.
                // The parent of that node is the successor.
                //
                // If a node is in the right subtree then it must be > than it's parent.
                // If parent is also in the right subtree of it's parent then
                //  node > parent > parent.parent and so on.
                // Now if the parent is in the left subtree then it must be < than it's parent.
                // node > parent but parent < parent.parent and node < parent.parent.
                // Everything in the parent's left subtree must be < node and everything in
                // the right subtree of parent.parent is > that itself and thus > node.
                // Thus parent.parent is the smallest item that is larger than node -- it is its successor.
                let mut node_parent = unsafe { (*node).parent };
                while let Some(parent) = node_parent {
                    let parent = parent.as_ptr();
                    unsafe {
                        match (*parent).left {
                            Some(left) if ptr::eq(node, left.as_ptr()) => break,
                            _ => {}
                        }
                    }
                    node = parent;
                    node_parent = unsafe { (*node).parent };
                }

                node_parent
            }
        }
    }

    pub fn predecessor<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q> + Eq,
        Q: Ord,
    {
        match self.get_raw(key) {
            Some(node) => unsafe {
                self.predecessor_core(node)
                    .map(|node| self.node_as_refs(node))
            },
            None => None,
        }
    }

    fn predecessor_core(&self, node: NonNull<Node<K, V>>) -> Option<NonNull<Node<K, V>>>
    where
        K: Eq,
    {
        //       ┌────────── 34 ─────────┐
        //       │                       │
        // ┌──── 2 ────┐                 58 ────┐
        // │           │                        │
        // 1      ┌─── 9 ────┐              ┌── 77 ──┐
        //        │          │              │        │
        //     ┌─ 6       ┌─ 20 ─┐      ┌─ 71 ─┐     82
        //     │          │      │      │      │
        //     5         12 ─┐   24    67      75
        //                   │
        //                   13

        let mut node = node.as_ptr();
        match unsafe { (*node).left } {
            // 2 -> 1, 9 -> 6, 20 -> 13, 77 -> 75
            Some(left) => unsafe { Some(self.max_of(left)) },
            _ => {
                // 12 -> 9, 58 -> 34, 67 -> 58
                // Move up the parents and find the first node which is the right child of it's parent.
                // The parent of that node is the predecessor.
                let mut node_parent = unsafe { (*node).parent };
                while let Some(parent) = node_parent {
                    let parent = parent.as_ptr();
                    unsafe {
                        match (*parent).right {
                            Some(right) if ptr::eq(node, right.as_ptr()) => break,
                            _ => {}
                        }
                    }
                    node = parent;
                    node_parent = unsafe { (*parent).parent };
                }

                node_parent
            }
        }
    }

    pub fn insert(&mut self, key: K, value: V)
    where
        K: Eq + Ord,
    {
        let mut new_node = Node {
            key,
            value,
            parent: None,
            left: None,
            right: None,
        };

        // Move left/right down the tree until we find empty slot
        let mut parent = None;
        let mut maybe_node = if self.is_empty() {
            None
        } else {
            Some(self.root)
        };
        while let Some(node) = maybe_node {
            parent = maybe_node;
            let node = node.as_ptr();
            unsafe {
                match (new_node.key).cmp(&(*node).key) {
                    std::cmp::Ordering::Less => maybe_node = (*node).left,
                    std::cmp::Ordering::Equal => {
                        (*node).key = new_node.key;
                        (*node).value = new_node.value;
                        return;
                    }
                    std::cmp::Ordering::Greater => maybe_node = (*node).right,
                }
            }
        }

        new_node.parent = parent;
        // new_node is a left, it cannot have left or right subtrees
        let new_node = Box::new(new_node);
        let new_node = unsafe { NonNull::new_unchecked(Box::into_raw(new_node)) };
        // update parent to point to the new node
        match parent {
            Some(parent) => {
                let parent = parent.as_ptr();
                unsafe {
                    if (*new_node.as_ptr()).key < (*parent).key {
                        (*parent).left = Some(new_node);
                    } else {
                        (*parent).right = Some(new_node);
                    }
                }
            }
            None => {
                self.root = new_node;
            }
        }

        self.len += 1;
    }

    pub fn delete<Q>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Eq + Ord,
    {
        self.get_raw(key).map(|node| self.delete_core(node))
    }

    fn delete_core(&mut self, node: NonNull<Node<K, V>>) -> (K, V) {
        //       ┌────────── 34 ─────────┐
        //       │                       │
        // ┌──── 2 ────┐                 58 ────┐
        // │           │                        │
        // 1      ┌─── 9 ────┐              ┌── 77 ──┐
        //        │          │              │        │
        //     ┌─ 6       ┌─ 20 ─┐      ┌─ 71 ─┐     82
        //     │          │      │      │      │
        //     5         12 ─┐   24    67      75
        //                   │
        //                   13

        let node_ptr = node.as_ptr();
        match unsafe { ((*node_ptr).left, (*node_ptr).right) } {
            (None, v @ Some(_)) | (v @ Some(_), None) | (None, v @ None) => unsafe {
                // `node` has no children or only one.
                // To remove `node` replace `node` with the it's child or `None`.
                // For example remove 1, 6, 12, 58 from tree above
                self.replace_subtree(node, v)
            },
            (Some(_), Some(right)) => unsafe {
                // We want to replace `node` with it's successor, that is the
                // next largest value in the tree. Since the `node` has right
                // child it's successor is the minimum of it's right subtree.
                // (See successor method for more details about it).
                let min = self.min_of(right);
                // Now we want to replace `node` with `min`.
                // There are two cases:
                //  a) `min` is the right child of `node`,
                //     in which case we simply replace `node` with `min`
                //     and reconnect `node.left` to `min.left`
                //     for example remove 20, 75, 77 from tree above
                //  b) `min` is not the right child of `node`
                //     in which case we first replace `min` by it's own right child.
                //     Note that `min` cannot have a left child, thus by this replace
                //     we are removing ´min´ from the tree so that we could replace
                //     `node` with `min`
                //     for example remove 9 from tree above, min will be 12

                if !ptr::eq(min.as_ptr(), right.as_ptr()) {
                    // b)
                    self.replace_subtree(min, (*min.as_ptr()).right);
                    // min will replace node, so min's right must point to node's right
                    (*min.as_ptr()).right = (*node.as_ptr()).right;
                    // atm `min.right.parent` points to the `node`, but it must point to `min`
                    (*(*min.as_ptr()).right.unwrap().as_ptr()).parent = Some(min);
                }
                self.replace_subtree(node, Some(min));
                // min replaced node, so min's left must point to node's left
                (*min.as_ptr()).left = (*node.as_ptr()).left;
                // atm `min.left.parent` points to the `node`, but it must point to `min`
                (*(*min.as_ptr()).left.unwrap().as_ptr()).parent = Some(min);
            },
        }

        let node = unsafe { Box::from_raw(node_ptr) };
        self.len -= 1;
        (node.key, node.value)
    }

    /// Replaces subtree `old` with subtree `new`
    unsafe fn replace_subtree(
        &mut self,
        old: NonNull<Node<K, V>>,
        new: Option<NonNull<Node<K, V>>>,
    ) {
        // We need to do two things:
        //  a) make the parent of `old` point to `new` instead of `old`,
        //     if `old` doesn't have parents it must have been the root which
        //     means that `new` will be the new root
        //  b) make `new` point to the parent of `old`

        unsafe {
            // a)
            match (*old.as_ptr()).parent {
                Some(parent) => {
                    let parent = parent.as_ptr();
                    match ((*parent).left, (*parent).right) {
                        (None, None) => unreachable!(),
                        // `parent` only had right subtree, must be where `old` was
                        (None, Some(_)) => (*parent).right = new,
                        // `parent` only had left subtree, must be where `old` was
                        (Some(_), None) => (*parent).left = new,
                        (Some(l), Some(_)) => {
                            // `parent` had both subtrees, compare the pointer to see which one `old` was
                            if ptr::eq(old.as_ptr(), l.as_ptr()) {
                                (*parent).left = new;
                            } else {
                                (*parent).right = new;
                            }
                        }
                    }
                }
                None => {
                    // `old` didn't have parents, it was the root, make `new` root
                    self.root = match new {
                        Some(new) => new,
                        None => NonNull::dangling(),
                    }
                }
            }

            // b)
            if let Some(new) = new {
                (*new.as_ptr()).parent = (*old.as_ptr()).parent;
            }
        }
    }

    #[inline]
    unsafe fn node_as_refs(&self, node: NonNull<Node<K, V>>) -> (&K, &V) {
        unsafe {
            let node = node.as_ptr();
            (&(*node).key, &(*node).value)
        }
    }

    #[inline]
    unsafe fn node_as_muts(&mut self, node: NonNull<Node<K, V>>) -> (&K, &mut V) {
        unsafe {
            let node = node.as_ptr();
            (&(*node).key, &mut (*node).value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let mut tree = BinarySearchTree::new();
        assert!(tree.is_empty());
        tree.insert(12, 12);
        assert_eq!(tree.len(), 1);
        tree.insert(5, 5);
        tree.insert(9, 9);
        tree.insert(2, 2);
        tree.insert(18, 18);
        assert_eq!(tree.len(), 5);
        tree.insert(15, 15);
        tree.insert(13, 13);
        tree.insert(17, 17);
        tree.insert(19, 19);

        println!("{tree:#?}")
    }

    #[test]
    fn inorder_for_each() {
        let mut tree = BinarySearchTree::new();
        assert!(tree.is_empty());

        let mut items = Vec::with_capacity(tree.len());
        tree.inorder_for_each(|k, _| items.push(*k));
        assert_eq!(&items, &[]);

        tree.insert(12, 12);
        assert_eq!(tree.len(), 1);
        tree.insert(5, 5);
        tree.insert(9, 9);
        tree.insert(2, 2);
        tree.insert(18, 18);
        assert_eq!(tree.len(), 5);
        tree.insert(15, 15);
        tree.insert(13, 13);
        tree.insert(17, 17);
        tree.insert(19, 19);

        let mut items = Vec::with_capacity(tree.len());
        tree.inorder_for_each(|k, _| items.push(*k));

        assert_eq!(&items, &[2, 5, 9, 12, 13, 15, 17, 18, 19]);
    }

    #[test]
    fn get() {
        let mut tree = BinarySearchTree::new();
        assert_eq!(tree.get(&4), None);

        tree.insert(12, 12);
        tree.insert(5, 5);
        tree.insert(9, 9);
        tree.insert(2, 2);
        tree.insert(18, 18);
        tree.insert(15, 15);
        tree.insert(13, 13);
        tree.insert(17, 17);
        tree.insert(19, 19);

        for it in [2, 5, 9, 18, 12, 15, 13, 17, 19] {
            assert_eq!(tree.get(&it), Some((&it, &it)));
        }
    }

    #[test]
    fn min_max() {
        let mut tree = BinarySearchTree::new();
        tree.insert(12, 12);
        tree.insert(5, 5);
        tree.insert(9, 9);
        tree.insert(2, 2);
        tree.insert(18, 18);
        tree.insert(15, 15);
        tree.insert(13, 13);
        tree.insert(17, 17);
        tree.insert(19, 19);

        assert_eq!(tree.min(), Some((&2, &2)));
        assert_eq!(tree.max(), Some((&19, &19)));
    }

    #[test]
    fn successor() {
        let mut tree = BinarySearchTree::new();
        tree.insert(12, 12);
        tree.insert(5, 5);
        tree.insert(9, 9);
        tree.insert(2, 2);
        tree.insert(18, 18);
        tree.insert(15, 15);
        tree.insert(13, 13);
        tree.insert(17, 17);
        tree.insert(19, 19);

        for it in [2, 5, 9, 12, 13, 15, 17, 18, 19].windows(2) {
            let key = it[0];
            let result = it[1];
            assert_eq!(tree.successor(&key), Some((&result, &result)));
        }

        assert_eq!(tree.successor(&19), None);
    }

    #[test]
    fn predecessor() {
        let mut tree = BinarySearchTree::new();
        tree.insert(12, 12);
        tree.insert(5, 5);
        tree.insert(9, 9);
        tree.insert(2, 2);
        tree.insert(18, 18);
        tree.insert(15, 15);
        tree.insert(13, 13);
        tree.insert(17, 17);
        tree.insert(19, 19);

        for it in [2, 5, 9, 12, 13, 15, 17, 18, 19].windows(2) {
            let key = it[1];
            let result = it[0];
            assert_eq!(tree.predecessor(&key), Some((&result, &result)));
        }

        assert_eq!(tree.predecessor(&2), None);
    }

    #[test]
    fn delete() {
        let mut tree = BinarySearchTree::new();
        assert_eq!(tree.get(&4), None);

        tree.insert(12, 12);
        tree.insert(5, 5);
        tree.insert(9, 9);
        tree.insert(2, 2);
        tree.insert(18, 18);
        tree.insert(15, 15);
        tree.insert(13, 13);
        tree.insert(17, 17);
        tree.insert(19, 19);

        for it in [2, 5, 9, 18, 12, 15, 13, 17, 19] {
            assert_eq!(tree.delete(&it), Some((it, it)));
        }
    }

    mod proptests {
        use std::collections::hash_map::RandomState;
        use std::collections::HashSet;

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
                let mut bst = BinarySearchTree::new();
                for v in &inserts {
                    bst.insert(*v, *v);
                }

                inserts.shuffle(&mut thread_rng());
                for key in inserts.iter().chain(access.iter()) {
                    assert_eq!(ref_hmap.get_key_value(key), bst.get(key));
                }
            }

            #[test]
            fn order(
                mut inserts in proptest::collection::vec(0..10000i32, 0..MAP_SIZE),
            ) {
                let mut bst = BinarySearchTree::new();
                for v in &inserts {
                    bst.insert(*v, *v);
                }

                let unique = HashSet::<_, RandomState>::from_iter(inserts.into_iter());
                let mut inserts: Vec<_> = unique.into_iter().collect();
                inserts.sort();

                let mut items = Vec::with_capacity(bst.len());
                bst.inorder_for_each(|k, _| items.push(*k));
                assert_eq!(&items, &inserts);
            }


            #[test]
            fn successor(
                inserts in proptest::collection::hash_set(0..10000i32, 0..MAP_SIZE),
            ) {
                let mut bst = BinarySearchTree::new();
                for v in &inserts {
                    bst.insert(*v, *v);
                }

                let mut items: Vec<_> = inserts.into_iter().collect();
                items.sort();

                for it in items.windows(2) {
                    let key = it[0];
                    let result = it[1];
                    assert_eq!(bst.successor(&key), Some((&result, &result)));
                }
            }

            #[test]
            fn predecessor(
                inserts in proptest::collection::hash_set(0..10000i32, 0..MAP_SIZE),
            ) {
                let mut bst = BinarySearchTree::new();
                for v in &inserts {
                    bst.insert(*v, *v);
                }

                let mut items: Vec<_> = inserts.into_iter().collect();
                items.sort();

                for it in items.windows(2) {
                    let key = it[1];
                    let result = it[0];
                    assert_eq!(bst.predecessor(&key), Some((&result, &result)));
                }
            }


            #[test]
            fn delete(
                mut inserts in proptest::collection::hash_set(0..10000i32, 0..MAP_SIZE),
                access in proptest::collection::vec(0..10000i32, 0..10)
            ) {
                let mut ref_hmap = std::collections::HashMap::<i32, i32, RandomState>::from_iter(inserts.iter().map(|v| (*v, *v)));
                let mut bst = BinarySearchTree::new();
                for v in &inserts {
                    bst.insert(*v, *v);
                }

                let mut inserts: Vec<_> = inserts.into_iter().collect();
                inserts.shuffle(&mut thread_rng());
                for key in inserts.iter().chain(access.iter()) {
                    assert_eq!(ref_hmap.remove_entry(key), bst.delete(key));
                }
            }

        );
    }
}
