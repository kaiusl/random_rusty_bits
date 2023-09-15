use core::fmt;
use std::borrow::Borrow;
use std::marker::PhantomData;
use std::mem::{self, MaybeUninit};
use std::ptr::{self, NonNull};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Black,
}

impl Color {
    /// Returns `true` if the color is [`Red`].
    ///
    /// [`Red`]: Color::Red
    #[must_use]
    fn is_red(&self) -> bool {
        matches!(self, Self::Red)
    }

    /// Returns `true` if the color is [`Black`].
    ///
    /// [`Black`]: Color::Black
    #[must_use]
    fn is_black(&self) -> bool {
        matches!(self, Self::Black)
    }
}

struct Node<K, V> {
    // key and value are uninit only for sentinel node which is used by the
    // delete routine, otherwise they must always be valid values
    key: MaybeUninit<K>,
    value: MaybeUninit<V>,
    color: Color,
    parent: Option<RawNode<K, V>>,
    left: Option<RawNode<K, V>>,
    right: Option<RawNode<K, V>>,
}

impl<K, V> fmt::Debug for Node<K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Node");
        f.field("key", unsafe { &self.key.assume_init_ref() })
            .field("value", unsafe { &self.value.assume_init_ref() })
            .field("color", &self.color);

        let mut dbg_opt_node = |name: &str, node: &Option<RawNode<K, V>>| match node {
            Some(node) => {
                let node = unsafe { node.as_ref() };
                f.field(name, unsafe {
                    &(
                        &node.key.assume_init_ref(),
                        &node.value.assume_init_ref(),
                        &node.color,
                    )
                });
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

/// Wrapper around `NonNull<Node<K, V>>` to provide convenient methods in order
/// to make the algorithms of RBTree much more readable.
#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
struct RawNode<K, V> {
    ptr: NonNull<Node<K, V>>,
}

impl<K, V> Clone for RawNode<K, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K, V> Copy for RawNode<K, V> {}

impl<K, V> RawNode<K, V> {
    fn dangling() -> Self {
        Self {
            ptr: NonNull::dangling(),
        }
    }

    fn from_node(node: Node<K, V>) -> Self {
        Self {
            ptr: unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(node))) },
        }
    }

    #[inline]
    fn as_ptr(&self) -> *mut Node<K, V> {
        self.ptr.as_ptr()
    }

    #[inline]
    unsafe fn as_ref<'a>(&self) -> &'a Node<K, V> {
        unsafe { self.ptr.as_ref() }
    }

    #[inline]
    unsafe fn as_mut<'a>(&mut self) -> &'a mut Node<K, V> {
        unsafe { self.ptr.as_mut() }
    }

    #[inline]
    unsafe fn key<'a>(&self) -> &'a K {
        unsafe { (*self.as_ptr()).key.assume_init_ref() }
    }

    #[inline]
    unsafe fn set_key_value(&mut self, key: K, value: V) {
        let ptr = self.as_ptr();
        unsafe {
            (*ptr).key = MaybeUninit::new(key);
            (*ptr).value = MaybeUninit::new(value);
        }
    }

    #[inline]
    unsafe fn as_refs<'a>(&self) -> (&'a K, &'a V) {
        let ptr = self.as_ptr();
        unsafe { ((*ptr).key.assume_init_ref(), (*ptr).value.assume_init_ref()) }
    }

    #[inline]
    unsafe fn as_muts<'a>(&mut self) -> (&'a K, &'a mut V) {
        let ptr = self.as_ptr();
        unsafe { ((*ptr).key.assume_init_ref(), (*ptr).value.assume_init_mut()) }
    }

    #[inline]
    unsafe fn parent(&self) -> Option<RawNode<K, V>> {
        unsafe { (*self.as_ptr()).parent }
    }

    #[inline]
    unsafe fn set_parent(&mut self, new_parent: Option<RawNode<K, V>>) {
        unsafe {
            (*self.as_ptr()).parent = new_parent;
        }
    }

    #[inline]
    unsafe fn right(&self) -> Option<RawNode<K, V>> {
        unsafe { (*self.as_ptr()).right }
    }

    #[inline]
    unsafe fn set_right(&mut self, new_right: Option<RawNode<K, V>>) {
        unsafe {
            (*self.as_ptr()).right = new_right;
        }
    }

    #[inline]
    unsafe fn left(&self) -> Option<RawNode<K, V>> {
        unsafe { (*self.as_ptr()).left }
    }

    #[inline]
    unsafe fn set_left(&mut self, new_left: Option<RawNode<K, V>>) {
        unsafe {
            (*self.as_ptr()).left = new_left;
        }
    }

    #[inline]
    unsafe fn color(&self) -> Color {
        unsafe { (*self.as_ptr()).color }
    }

    #[inline]
    unsafe fn set_color(&mut self, new_color: Color) {
        unsafe { (*self.as_ptr()).color = new_color }
    }

    #[inline]
    unsafe fn pos(&self) -> NodePos {
        let ptr = self.as_ptr();
        match unsafe { (*ptr).parent } {
            Some(p) => match unsafe { (p.left(), p.right()) } {
                (None, None) => unreachable!(),
                (None, Some(_)) => NodePos::Right,
                (Some(_), None) => NodePos::Left,
                (Some(left), Some(right)) => {
                    if ptr::eq(ptr, left.as_ptr()) {
                        NodePos::Left
                    } else {
                        assert!(ptr::eq(ptr, right.as_ptr()));
                        NodePos::Right
                    }
                }
            },
            None => NodePos::Root,
        }
    }

    #[inline]
    unsafe fn grand_parent(&self) -> Option<RawNode<K, V>> {
        unsafe { self.parent().and_then(|p| p.parent()) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodePos {
    Root,
    Left,
    Right,
}

struct RedBlackTree<K, V> {
    root: RawNode<K, V>,
    len: usize,
    // Sentinel value used by delete routine
    sentinel: RawNode<K, V>,
    marker: PhantomData<Box<Node<K, V>>>,
}

impl<K, V> Drop for RedBlackTree<K, V> {
    fn drop(&mut self) {
        if self.len == 0 {
            let _: Box<Node<K, V>> = unsafe { Box::from_raw(self.sentinel.as_ptr()) };
            return;
        }

        // TODO: handle panics in `K::drop` or `V::drop`

        unsafe fn inner<K, V>(node: RawNode<K, V>) {
            if let Some(l) = unsafe { node.left() } {
                unsafe { inner(l) };
            }
            if let Some(r) = unsafe { node.right() } {
                unsafe { inner(r) };
            }
            let _: Box<Node<K, V>> = unsafe { Box::from_raw(node.as_ptr()) };
        }

        self.len = 0;
        unsafe { inner(self.root) };
        let _: Box<Node<K, V>> = unsafe { Box::from_raw(self.sentinel.as_ptr()) };
    }
}

impl<K, V> fmt::Debug for RedBlackTree<K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct TreeDebug<'a, K, V> {
            root: RawNode<K, V>,
            marker: PhantomData<&'a Node<K, V>>,
        }

        impl<K, V> fmt::Debug for TreeDebug<'_, K, V>
        where
            K: fmt::Debug,
            V: fmt::Debug,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut f = f.debug_list();

                let mut func = |node: RawNode<K, V>| {
                    let node = unsafe { node.as_ref() };
                    f.entry(&node);
                };

                unsafe { RedBlackTree::inorder_for_each_core(self.root, &mut func) };
                f.finish()
            }
        }

        let mut f = f.debug_struct("RedBlackTree");
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

impl<K, V> RedBlackTree<K, V> {
    pub fn new() -> Self {
        Self {
            root: RawNode::dangling(),
            len: 0,
            sentinel: RawNode::from_node(Node {
                key: MaybeUninit::uninit(),
                value: MaybeUninit::uninit(),
                color: Color::Black,
                parent: None,
                left: None,
                right: None,
            }),
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

        let mut f = |mut node: RawNode<K, V>| {
            let node = unsafe { node.as_mut() };
            unsafe { f(node.key.assume_init_mut(), node.value.assume_init_mut()) }
        };
        unsafe { Self::inorder_for_each_core(self.root, &mut f) }
    }

    unsafe fn inorder_for_each_core<F>(node: RawNode<K, V>, f: &mut F)
    where
        F: FnMut(RawNode<K, V>),
    {
        if let Some(l) = unsafe { node.left() } {
            unsafe { Self::inorder_for_each_core(l, f) };
        }
        f(node);
        if let Some(r) = unsafe { node.right() } {
            unsafe { Self::inorder_for_each_core(r, f) };
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.get_raw(key).map(|node| unsafe { node.as_refs() })
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<(&K, &mut V)>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.get_raw(key).map(|mut node| unsafe { node.as_muts() })
    }

    fn get_raw<Q>(&self, key: &Q) -> Option<RawNode<K, V>>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        if self.is_empty() {
            return None;
        }

        let mut x = self.root;
        loop {
            match key.cmp(unsafe { (*x.as_ptr()).key.assume_init_ref().borrow() }) {
                std::cmp::Ordering::Less => match unsafe { x.left() } {
                    Some(left) => x = left,
                    None => break,
                },
                std::cmp::Ordering::Equal => return Some(x),
                std::cmp::Ordering::Greater => match unsafe { x.right() } {
                    Some(right) => x = right,
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
        unsafe { Some(min.as_refs()) }
    }

    unsafe fn min_of(&self, root: RawNode<K, V>) -> RawNode<K, V> {
        let mut x = root;
        while let Some(left) = unsafe { x.left() } {
            x = left;
        }

        x
    }

    pub fn max(&self) -> Option<(&K, &V)> {
        if self.is_empty() {
            return None;
        }
        let max = unsafe { self.max_of(self.root) };
        unsafe { Some(max.as_refs()) }
    }

    unsafe fn max_of(&self, root: RawNode<K, V>) -> RawNode<K, V> {
        let mut x = root;
        while let Some(right) = unsafe { x.right() } {
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
            Some(node) => unsafe { self.successor_core(node).map(|node| node.as_refs()) },
            None => None,
        }
    }

    unsafe fn successor_core(&self, mut node: RawNode<K, V>) -> Option<RawNode<K, V>>
    where
        K: Eq,
    {
        //       +---------- 34 ---------+
        //       |                       |
        // +---- 2 ----+                 58 ----+
        // |           |                        |
        // 1      +--- 9 ----+              +-- 77 --+
        //        |          |              |        |
        //     +- 6       +- 20 -+      +- 71 -+     82
        //     |          |      |      |      |
        //     5         12 -+   24    67      75
        //                   |
        //                   13

        match unsafe { node.right() } {
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
                let mut node_parent = unsafe { node.parent() };
                while let Some(parent) = node_parent {
                    unsafe {
                        match parent.left() {
                            Some(left) if ptr::eq(node.as_ptr(), left.as_ptr()) => break,
                            _ => {}
                        }
                    }
                    node = parent;
                    node_parent = unsafe { node.parent() };
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
            Some(node) => unsafe { self.predecessor_core(node).map(|node| node.as_refs()) },
            None => None,
        }
    }

    fn predecessor_core(&self, mut node: RawNode<K, V>) -> Option<RawNode<K, V>>
    where
        K: Eq,
    {
        //       +---------- 34 ---------+
        //       |                       |
        // +---- 2 ----+                 58 ----+
        // |           |                        |
        // 1      +--- 9 ----+              +-- 77 --+
        //        |          |              |        |
        //     +- 6       +- 20 -+      +- 71 -+     82
        //     |          |      |      |      |
        //     5         12 -+   24    67      75
        //                   |
        //                   13
        match unsafe { node.left() } {
            // 2 -> 1, 9 -> 6, 20 -> 13, 77 -> 75
            Some(left) => unsafe { Some(self.max_of(left)) },
            _ => {
                // 12 -> 9, 58 -> 34, 67 -> 58
                // Move up the parents and find the first node which is the right child of it's parent.
                // The parent of that node is the predecessor.
                let mut node_parent = unsafe { node.parent() };
                while let Some(parent) = node_parent {
                    unsafe {
                        match parent.right() {
                            Some(right) if ptr::eq(node.as_ptr(), right.as_ptr()) => break,
                            _ => {}
                        }
                    }
                    node = parent;
                    node_parent = unsafe { node.parent() };
                }

                node_parent
            }
        }
    }

    fn rotate_left(&mut self, mut node: RawNode<K, V>) {
        //    p                       p
        //    |                       |
        // +-node-+               +-right-+
        // |      |      -->      |       |
        // a  +-right-+       +-node-+    c
        //    |       |       |      |
        //    b       c       a      b
        // where a, b, c can be any subtrees
        unsafe {
            if let Some(mut right) = node.right() {
                // attach b to node
                let b = right.left();
                node.set_right(b);
                if let Some(mut new_right) = node.right() {
                    new_right.set_parent(Some(node));
                }

                // attach right to parent
                let parent = node.parent();
                right.set_parent(parent);
                match node.pos() {
                    NodePos::Root => self.root = right,
                    NodePos::Left => parent.unwrap().set_left(Some(right)),
                    NodePos::Right => parent.unwrap().set_right(Some(right)),
                }

                // attach node to right
                right.set_left(Some(node));
                node.set_parent(Some(right));
            }
        }
    }

    fn rotate_right(&mut self, mut node: RawNode<K, V>) {
        //         p              p
        //         |              |
        //     +-node-+       +-left-+
        //     |      |       |      |
        // +-left-+   c  -->  a  +-node-+
        // |      |              |      |
        // a      b              b      c
        // where a, b, c can be any subtrees

        unsafe {
            if let Some(mut left) = node.left() {
                // attach b to node
                let b = left.right();
                node.set_left(b);
                if let Some(mut new_left) = node.left() {
                    new_left.set_parent(Some(node));
                }

                // attach left to parent
                let parent = node.parent();
                left.set_parent(parent);
                match node.pos() {
                    NodePos::Root => self.root = left,
                    NodePos::Left => parent.unwrap().set_left(Some(left)),
                    NodePos::Right => parent.unwrap().set_right(Some(left)),
                }

                // attach node to left
                left.set_right(Some(node));
                node.set_parent(Some(left));
            }
        }
    }

    pub fn insert(&mut self, key: K, value: V)
    where
        K: Eq + Ord,
    {
        let mut new_node = Node {
            key: MaybeUninit::new(key),
            value: MaybeUninit::new(value),
            color: Color::Red,
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
        while let Some(mut node) = maybe_node {
            parent = maybe_node;
            unsafe {
                match (new_node.key.assume_init_ref()).cmp(node.key()) {
                    std::cmp::Ordering::Less => maybe_node = node.left(),
                    std::cmp::Ordering::Equal => {
                        node.set_key_value(
                            new_node.key.assume_init(),
                            new_node.value.assume_init(),
                        );
                        return;
                    }
                    std::cmp::Ordering::Greater => maybe_node = node.right(),
                }
            }
        }

        new_node.parent = parent;
        // new_node is a leaf, it cannot have left or right subtrees
        let new_node = RawNode::from_node(new_node);
        // update parent to point to the new node
        match parent {
            Some(mut parent) => unsafe {
                if new_node.key() < parent.key() {
                    parent.set_left(Some(new_node));
                } else {
                    parent.set_right(Some(new_node));
                }
            },
            None => self.root = new_node,
        }

        self.len += 1;
        self.insert_fixup(new_node);
    }

    fn insert_fixup(&mut self, new_node: RawNode<K, V>) {
        let mut node = new_node;
        unsafe {
            loop {
                match node.parent() {
                    Some(mut parent) if parent.color().is_red() => {
                        debug_assert!(node.color().is_red());
                        // red-black properties are violated because red parent has a red child
                        //
                        // Note that there is only one violation at this point.
                        // At first iteration it's the new_node and it's parent.
                        // If we take the "red uncle" branch then at next iteration it will be
                        // the grand_parent and it's parent that violate the red-black properties.
                        // If we take the other branch, there will be no more iterations as that
                        // will result in a black parent.

                        match parent.pos() {
                            NodePos::Root => unreachable!(),
                            NodePos::Left => {
                                // grand_parent must exist because parent is red and
                                // thus not root as root is always black
                                let mut grand_parent = parent.parent().unwrap();
                                let uncle = grand_parent.right();
                                debug_assert!(grand_parent.color().is_black());

                                match uncle {
                                    Some(mut uncle) if uncle.color().is_red() => {
                                        //     +--- gp:b ---+               +--- gp:r ---+
                                        //     |            |               |            |
                                        //  + p:r +      + u:r +   -->   + p:b +      + u:b +
                                        //  |     |      |     |         |     |      |     |
                                        // n:r   a:b    b:b   c:b       n:r   a:b    b:b   c:b
                                        // (a, b, c can be any subtrees)
                                        //
                                        // at first thought we could color new node `n` black but that would
                                        // increase the black height by one on that leaf but not on other leaves
                                        // which is again in violation to red-black properties.
                                        // Instead color parent and uncle black and grandparent red, which keeps
                                        // the black height unchanged.
                                        // Now the grand parent may also have a red parent, but we can simply
                                        // repeat the process as if the grand parent was the new added node.
                                        parent.set_color(Color::Black);
                                        uncle.set_color(Color::Black);
                                        grand_parent.set_color(Color::Red);
                                        node = grand_parent;
                                    }
                                    _ => {
                                        if let NodePos::Right = node.pos() {
                                            //       +-- gp:b --+                 +-- gp:b --+
                                            //       |          |                 |          |
                                            //  +-- p:r --+    u:b  -->       +- n:r --+    u:b
                                            //  |         |                   |        |
                                            // a:b    +- n:r -+           +- p:r -+   c:b
                                            //        |       |           |       |
                                            //       b:b     c:b         a:b     b:b
                                            // (a, b, c, u can be any subtrees)
                                            //
                                            // left rotate parent and swap node and parent pointers so we match the case below
                                            self.rotate_left(parent);
                                            mem::swap(&mut parent, &mut node);
                                        }

                                        //           +-- gp:b --+            +----- p:b -----+
                                        //           |          |            |               |
                                        //      +-- p:r --+    u:b  -->   +- n:r -+     +- gp:r -+
                                        //      |         |               |       |     |        |
                                        //  +- n:r -+    c:b             a:b     b:b   c:b      u:b
                                        //  |       |
                                        // a:b     b:b
                                        //
                                        // (a, b, c, u can be any subtrees)
                                        //
                                        // Note that this will fix the one violation we had and thus the whole tree
                                        // is again a proper red-black tree.

                                        parent.set_color(Color::Black);
                                        grand_parent.set_color(Color::Red);
                                        self.rotate_right(grand_parent);
                                    }
                                }
                            }
                            NodePos::Right => {
                                // same as Left branch but left/right are switched
                                let mut grand_parent = parent.parent().unwrap();
                                let uncle = grand_parent.left();

                                match uncle {
                                    Some(mut uncle) if uncle.color().is_red() => {
                                        parent.set_color(Color::Black);
                                        uncle.set_color(Color::Black);
                                        grand_parent.set_color(Color::Red);
                                        node = grand_parent;
                                    }
                                    _ => {
                                        if let NodePos::Left = node.pos() {
                                            self.rotate_right(parent);
                                            mem::swap(&mut parent, &mut node);
                                        }

                                        parent.set_color(Color::Black);
                                        grand_parent.set_color(Color::Red);
                                        self.rotate_left(grand_parent);
                                    }
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }

            self.root.set_color(Color::Black);
        }
    }

    fn insert_bst(&mut self, key: K, value: V)
    where
        K: Eq + Ord,
    {
        let mut new_node = Node {
            key: MaybeUninit::new(key),
            value: MaybeUninit::new(value),
            color: Color::Black,
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
        while let Some(mut node) = maybe_node {
            parent = maybe_node;
            unsafe {
                match (new_node.key.assume_init_ref()).cmp(node.key()) {
                    std::cmp::Ordering::Less => maybe_node = node.left(),
                    std::cmp::Ordering::Equal => {
                        node.set_key_value(
                            new_node.key.assume_init(),
                            new_node.value.assume_init(),
                        );
                        return;
                    }
                    std::cmp::Ordering::Greater => maybe_node = node.right(),
                }
            }
        }

        new_node.parent = parent;
        // new_node is a left, it cannot have left or right subtrees
        let new_node = RawNode::from_node(new_node);
        // update parent to point to the new node
        match parent {
            Some(mut parent) => unsafe {
                if new_node.key() < parent.key() {
                    parent.set_left(Some(new_node));
                } else {
                    parent.set_right(Some(new_node));
                }
            },
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

    fn delete_core(&mut self, node: RawNode<K, V>) -> (K, V) {
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

        unsafe {
            let mut to_remove = node;
            let mut to_remove_orig_color = to_remove.color();
            // Node that replaces the removed node
            let mut replacement: RawNode<K, V>;
            match (node.left(), node.right()) {
                (None, v @ Some(_)) | (v @ Some(_), None) | (None, v @ None) => unsafe {
                    // `node` has no children or only one.
                    // To remove `node` replace `node` with the its child or `None`.
                    // For example remove 1, 6, 12, 58 from tree above

                    self.replace_subtree(node, v);
                    replacement = v.unwrap_or(self.sentinel);
                    //println!("1");
                },
                (Some(_), Some(right)) => {
                    //println!("2");
                    // We want to replace `node` with it's successor, that is the
                    // next largest value in the tree. Since the `node` has right
                    // child it's successor is the minimum of it's right subtree.
                    // (See successor method for more details about it).
                    to_remove = self.min_of(right);
                    to_remove_orig_color = to_remove.color();
                    replacement = to_remove.right().unwrap_or(self.sentinel);

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

                    if ptr::eq(to_remove.as_ptr(), right.as_ptr()) {
                        replacement.set_parent(Some(to_remove));
                    } else {
                        // b)
                        self.replace_subtree(to_remove, to_remove.right());
                        //x_parent = Some(y);
                        // min will replace node, so min's right must point to node's right
                        to_remove.set_right(node.right());
                        // atm `min.right.parent` points to the `node`, but it must point to `min`
                        to_remove.right().unwrap().set_parent(Some(to_remove));
                    }
                    self.replace_subtree(node, Some(to_remove));
                    // min replaced node, so min's left must point to node's left
                    to_remove.set_left(node.left());
                    // atm `min.left.parent` points to the `node`, but it must point to `min`
                    to_remove.left().unwrap().set_parent(Some(to_remove));
                    to_remove.set_color(node.color());
                }
            }

            if to_remove_orig_color.is_black() {
                self.delete_fixup(replacement);
            }
            self.sentinel.set_parent(None);
            self.sentinel.set_color(Color::Black);

            let node = Box::from_raw(node.as_ptr());
            self.len -= 1;
            (node.key.assume_init(), node.value.assume_init())
        }
    }

    fn delete_fixup(&mut self, mut x: RawNode<K, V>) {
        // x points to the place where we removed the node which was black
        // x itself can be either red or black.
        //
        // If x is red then we moved one red node up from down the tree and
        // thus the number of black nodes on subtrees of x didn't change.
        // But we removed one black node from one of the subtrees of x's parent.
        // We can simply fix it by coloring x black again.
        //
        // If x is black and root then we simply color it black.
        // If x was not the root we enter the loop ...
        unsafe {
            while x.color().is_black() && x.parent().is_some() {
                // ...
                // At this point following holds:
                // * x is not root
                // * x is doubly black
                //   This means that "same black height" property holds but we may
                //   violate "node must be either red or black", "root is black"
                //   or "red parent has black children" properties.
                // * x can be self.sentinel or a proper node
                // * x must have a sibling
                //   If x == self.sentinel x.parent has one child,
                //   otherwise parent must have both children.
                //
                //   This is because otherwise the black heights from x.parent
                //   down to the leaves cannot be equal. The shortest black height
                //   from x.p -> x is 2 if x == self.sentinel. If the sibling is self.sentinel then
                //   there is no way for the black height from x.p -> x_sibling to equal 2.
                //   This violates the "same black height" red-black property (which holds) and
                //   thus x_sibling cannot be self.sentinel.
                let mut x_parent = x.parent().unwrap();
                debug_assert!(
                    x_parent.left().is_some() || x_parent.right().is_some(),
                    "x.parent should have at least one child"
                );

                let is_x_sentinel = ptr::eq(x.as_ptr(), self.sentinel.as_ptr());
                // Note that if is_x_sentinel == true then x.pos() doesn't return the real
                // position of x but the position of which child the parent has.
                // So (x.pos() == Right, is_x_sentinel == true) says that x.parent has only a right child.
                match (x.pos(), is_x_sentinel) {
                    (NodePos::Root, _) => unreachable!(),
                    (NodePos::Left, false) | (NodePos::Right, true) => {
                        // x in the left child and the parent has both children
                        // or x is sentinel and parent has only a right child
                        let mut x_sibling = x_parent.right().unwrap();
                        if cfg!(debug_assertions) {
                            if is_x_sentinel {
                                assert!(x_parent.left().is_none());
                                assert!(x_parent.right().is_some());
                            } else {
                                assert!(x_parent.left().is_some());
                                assert!(x_parent.right().is_some());
                            }
                            assert!(
                                !ptr::eq(x_sibling.as_ptr(), self.sentinel.as_ptr()),
                                "w should not be a self.null"
                            );
                            assert!(
                                !ptr::eq(x_sibling.as_ptr(), x.as_ptr()),
                                "w should not be equal to x"
                            );
                        }

                        if x_sibling.color().is_red() {
                            //println!("L: case 1");
                            //
                            //     ┌─── p:b ───┐                ┌─── p:r ───┐                    ┌─── s:b ───┐
                            //     │           │                │           │                    │           │
                            // ┌─ x:b ─┐   ┌─ s:r ─┐   ──►  ┌─ x:b ─┐   ┌─ s:b ─┐   ──►      ┌─ p:r ─┐      d:b
                            // │       │   │       │        │       │   │       │            │       │
                            // a       b  c:b     d:b       a       b  c:b     d:b       ┌─ x:b ─┐  c:b
                            //                                                           │       │
                            //                                                           a       b
                            // As a result turns into case 2, 3 or 4 depending on the color of node c's children.
                            // We haven't created any more issues but all paths through x still have a missing black node.
                            // However x has gained a red parent and the cases 2, 3 or 4 below will fix the tree.

                            // Parent must be black because we haven't changed the color of parent and sibling yet
                            // and thus parent must be black to have a red child.
                            //
                            // The sibling must have both children to satisfy the "same black height property".
                            // The argument goes same as the one above the loop, except if the sibling is red then
                            // the black nodes must be it's children.
                            debug_assert!(x_parent.color().is_black());
                            debug_assert!(x_sibling.left().is_some());
                            debug_assert!(x_sibling.right().is_some());
                            x_sibling.set_color(Color::Black);
                            x_parent.set_color(Color::Red);
                            self.rotate_left(x_parent);
                            x_sibling = x_parent.right().unwrap();
                        }

                        debug_assert!(x_sibling.color().is_black());

                        let sibling_left_color =
                            x_sibling.left().map(|n| n.color()).unwrap_or(Color::Black);
                        let sibling_right_color =
                            x_sibling.right().map(|n| n.color()).unwrap_or(Color::Black);

                        if sibling_left_color.is_black() && sibling_right_color.is_black() {
                            // println!("L: case 2");
                            // Take off the extra black from x and x's sibling and put it on x's parent.
                            // That is move the extra black up the tree until we can totally remove it in next iterations.
                            //
                            //     ┌─── p:c ───┐                ┌─── p:c ───┐
                            //     │           │                │           │
                            // ┌─ x:b ─┐   ┌─ s:b ─┐   ──►  ┌─ x:b ─┐   ┌─ s:r ─┐
                            // │       │   │       │        │       │   │       │
                            // a       b  c:b     d:b       a       b  c:b     d:b
                            //
                            // If we came here from case 1 then the loop will terminate (also if the parent was just red)
                            // because parent is red and thus x will be red at next iteration.
                            // The issue with red parent having a red child will be fixed after the loop
                            // by coloring x black.
                            // After coloring parent black all paths going through x will gain an extra black node
                            // that was taken away by the remove operations. All paths through node s keep the same number
                            // of black nodes because we took one away from s but added one to p.

                            x_sibling.set_color(Color::Red);
                            x = x_parent;
                        } else {
                            if sibling_left_color.is_red() {
                                //println!("L: case 3");
                                //
                                //    ┌───── p:c ─────┐                ┌───── p:c ─────┐                ┌─── p:c ───┐
                                //    │               │                │               │                │           │
                                // ┌─ x:b ─┐      ┌─ s:b ─┐   ──►  ┌─ x:b ─┐       ┌─ s:r ─┐   ──►  ┌─ x:b ─┐   ┌─ c:b ─┐
                                // │       │      │       │        │       │       │       │        │       │   │       │
                                // a       b  ┌─ c:r ─┐  d:b       a       b   ┌─ c:b ─┐   d:b      a       b   e   ┌─ s:r ─┐
                                //            │       │                        │       │                            │       │
                                //            e       f                        e       f                            f      d:b
                                //
                                // Turns into case 4, all paths to leaves keep the same number of black nodes as was before,
                                // that is paths through x still have one missing black node compared to other paths.

                                // sibling must have a left child because it is red
                                x_sibling.left().unwrap().set_color(Color::Black);
                                x_sibling.set_color(Color::Red);
                                self.rotate_right(x_sibling);
                                x_sibling = x_parent.right().unwrap();
                            }

                            // println!("L: case 4");
                            //
                            //     ┌─── p:c ───┐                ┌─── p:b ───┐                     ┌── s:c ──┐
                            //     │           │                │           │                     │         │
                            // ┌─ x:b ─┐   ┌─ s:b ─┐   ──►  ┌─ x:b ─┐   ┌─ s:c ─┐   ──►       ┌─ p:b ─┐    d:b
                            // │       │   │       │        │       │   │       │             │       │
                            // a       b  c:b     d:r       a       b  c:b     d:b       ┌─ x:b ─┐   c:b
                            //                                                           │       │
                            //                                                           a       b
                            //
                            // This will terminate the loop because the root of the tree is the same color as it was,
                            // (so it must match with red-black tree properties)
                            // but x has an extra black ancestor (either p became black or s was added as black grandparent).
                            // Thus the paths going through x have gained one extra black node which was missing.
                            // The lost black on paths through d is accounted by recoloring d black. All other path
                            // keep the number of black nodes.

                            x_sibling.set_color(x_parent.color());
                            x_parent.set_color(Color::Black);
                            // sibling must have a right child because it is red
                            x_sibling.right().unwrap().set_color(Color::Black);
                            self.rotate_left(x_parent);
                            break;
                        }
                    }
                    (NodePos::Left, true) | (NodePos::Right, false) => {
                        // x is the right child and the parent has both children
                        // or x is sentinel and parent has only a left child

                        let mut x_sibling = x_parent.left().unwrap();
                        if cfg!(debug_assertions) {
                            if is_x_sentinel {
                                assert!(x_parent.left().is_some());
                                assert!(x_parent.right().is_none());
                            } else {
                                assert!(x_parent.left().is_some());
                                assert!(x_parent.right().is_some());
                            }

                            assert!(!ptr::eq(x_sibling.as_ptr(), self.sentinel.as_ptr()));
                            assert!(!ptr::eq(x_sibling.as_ptr(), x.as_ptr()));
                        }

                        if x_sibling.color().is_red() {
                            //println!("R: case 1");
                            x_sibling.set_color(Color::Black);
                            x_parent.set_color(Color::Red);
                            self.rotate_right(x_parent);
                            x_sibling = x_parent.left().unwrap();
                        }

                        let sibling_left_color =
                            x_sibling.left().map(|n| n.color()).unwrap_or(Color::Black);
                        let sibling_right_color =
                            x_sibling.right().map(|n| n.color()).unwrap_or(Color::Black);

                        if sibling_left_color.is_black() && sibling_right_color.is_black() {
                            // println!("R: case 2");
                            x_sibling.set_color(Color::Red);
                            x = x_parent;
                        } else {
                            if sibling_left_color.is_black() {
                                // println!("R: case 3");
                                x_sibling.right().unwrap().set_color(Color::Black);
                                x_sibling.set_color(Color::Red);
                                self.rotate_left(x_sibling);
                                x_sibling = x_parent.left().unwrap();
                            }

                            // println!("R:case 4");
                            x_sibling.set_color(x_parent.color());
                            x_parent.set_color(Color::Black);
                            x_sibling.left().unwrap().set_color(Color::Black);
                            self.rotate_right(x_parent);
                            break;
                        }
                    }
                }
            }
            x.set_color(Color::Black);
        }
    }

    /// Replaces subtree `old` with subtree `new`
    unsafe fn replace_subtree(&mut self, old: RawNode<K, V>, new: Option<RawNode<K, V>>) {
        // We need to do two things:
        //  a) make the parent of `old` point to `new` instead of `old`,
        //     if `old` doesn't have parents it must have been the root which
        //     means that `new` will be the new root
        //  b) make `new` point to the parent of `old`

        unsafe {
            // a)
            match old.pos() {
                NodePos::Root => {
                    self.root = match new {
                        Some(new) => new,
                        None => RawNode::dangling(),
                    }
                }
                NodePos::Left => old.parent().unwrap().set_left(new),
                NodePos::Right => old.parent().unwrap().set_right(new),
            }

            // b)
            if let Some(mut new) = new {
                new.set_parent(old.parent());
            } else {
                self.sentinel.set_parent(old.parent());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestNode {
        key: i32,
        parent_k: Option<i32>,
        left_k: Option<Box<TestNode>>,
        right_k: Option<Box<TestNode>>,
    }

    impl TestNode {
        fn leaf(key: i32, parent: i32) -> Self {
            Self {
                key,
                parent_k: Some(parent),
                left_k: None,
                right_k: None,
            }
        }
    }

    impl PartialEq<Node<i32, i32>> for TestNode {
        fn eq(&self, other: &Node<i32, i32>) -> bool {
            if self.key != unsafe { other.key.assume_init() } {
                return false;
            }
            match (&other.parent, &self.parent_k) {
                (None, None) => {}
                (None, Some(_)) => return false,
                (Some(_), None) => return false,
                (Some(actual), Some(expected)) => {
                    if unsafe { expected != actual.key() } {
                        return false;
                    }
                }
            }

            match (&other.left, &self.left_k) {
                (None, None) => {}
                (None, Some(_)) => return false,
                (Some(_), None) => return false,
                (Some(actual), Some(expected)) => {
                    if unsafe { !expected.eq(actual.as_ref()) } {
                        return false;
                    }
                }
            }

            match (&other.right, &self.right_k) {
                (None, None) => {}
                (None, Some(_)) => return false,
                (Some(_), None) => return false,
                (Some(actual), Some(expected)) => {
                    if unsafe { !expected.eq(actual.as_ref()) } {
                        return false;
                    }
                }
            }

            true
        }
    }

    fn assert_red_blackness(root: &Node<i32, i32>) {
        assert_eq!(root.color, Color::Black, "root must be black");
        fn inner(node: &Node<i32, i32>) {
            if node.color.is_red() {
                assert!(
                    node.left
                        .map(|l| unsafe { l.color() }.is_black())
                        .unwrap_or(true),
                    "left child of red node must be black : {:#?}",
                    node
                );
                assert!(
                    node.right
                        .map(|l| unsafe { l.color() }.is_black())
                        .unwrap_or(true),
                    "right child of red node must be black : {:#?}",
                    node
                );
            }

            if let Some(left) = node.left {
                inner(unsafe { left.as_ref() })
            }
            if let Some(right) = node.right {
                inner(unsafe { right.as_ref() })
            }
        }

        inner(root);

        // Find the black height by going down the left subtrees.
        // The black height must be the same in all path taken,
        // so choose the simplest one to determine the expected value.
        let mut black_count = 1;
        let mut node = root;
        while let Some(left) = node.left {
            if unsafe { left.color().is_black() } {
                black_count += 1;
            }

            node = unsafe { left.as_ref() };
        }

        fn assert_black_height(node: &Node<i32, i32>, expected_black_count: u64) {
            if node.left.is_none() && node.right.is_none() {
                // node is leaf, count the number of black nodes back up to the root
                // this must be the same for all leaves
                let mut black_count = node.color.is_black() as u64;
                let mut node = node;
                while let Some(parent) = node.parent {
                    if unsafe { parent.color().is_black() } {
                        black_count += 1;
                    }
                    node = unsafe { parent.as_ref() };
                }
                assert_eq!(expected_black_count, black_count);
            }

            if let Some(l) = node.left {
                unsafe { assert_black_height(l.as_ref(), expected_black_count) };
            }
            if let Some(r) = node.right {
                unsafe { assert_black_height(r.as_ref(), expected_black_count) };
            }
        }

        assert_black_height(root, black_count);
    }

    #[test]
    fn test() {
        let mut tree = RedBlackTree::new();
        assert!(tree.is_empty());
        tree.insert(12, 12);
        assert_eq!(tree.len(), 1);
        tree.insert(15, 15);
        assert_red_blackness(unsafe { tree.root.as_ref() });
        tree.insert(14, 14);
        assert_red_blackness(unsafe { tree.root.as_ref() });
        tree.insert(16, 16);
        assert_red_blackness(unsafe { tree.root.as_ref() });
        println!("{tree:#?}");
    }

    #[test]
    fn test_rotate_roundtrip() {
        let mut tree = RedBlackTree::new();
        assert!(tree.is_empty());
        tree.insert_bst(12, 12);
        tree.insert_bst(9, 9);
        assert_eq!(tree.len(), 2);
        tree.insert_bst(15, 15);
        tree.insert_bst(14, 14);
        tree.insert_bst(16, 16);

        let expected0 = TestNode {
            key: 12,
            parent_k: None,
            left_k: Some(Box::new(TestNode::leaf(9, 12))),
            right_k: Some(Box::new(TestNode {
                key: 15,
                parent_k: Some(12),
                left_k: Some(Box::new(TestNode::leaf(14, 15))),
                right_k: Some(Box::new(TestNode::leaf(16, 15))),
            })),
        };
        assert_eq!(&expected0, unsafe { tree.root.as_ref() });

        tree.rotate_left(tree.root);
        let expected1 = TestNode {
            key: 15,
            parent_k: None,
            left_k: Some(Box::new(TestNode {
                key: 12,
                parent_k: Some(15),
                left_k: Some(Box::new(TestNode::leaf(9, 12))),
                right_k: Some(Box::new(TestNode::leaf(14, 12))),
            })),
            right_k: Some(Box::new(TestNode::leaf(16, 15))),
        };
        assert_eq!(&expected1, unsafe { tree.root.as_ref() });

        tree.rotate_left(tree.root);
        let expected2 = TestNode {
            key: 16,
            parent_k: None,
            left_k: Some(Box::new(TestNode {
                key: 15,
                parent_k: Some(16),
                left_k: Some(Box::new(TestNode {
                    key: 12,
                    parent_k: Some(15),
                    left_k: Some(Box::new(TestNode::leaf(9, 12))),
                    right_k: Some(Box::new(TestNode::leaf(14, 12))),
                })),
                right_k: None,
            })),
            right_k: None,
        };
        assert_eq!(&expected2, unsafe { tree.root.as_ref() });

        let node = tree.get_raw(&12).unwrap();
        tree.rotate_left(node);
        let expected3 = TestNode {
            key: 16,
            parent_k: None,
            left_k: Some(Box::new(TestNode {
                key: 15,
                parent_k: Some(16),
                left_k: Some(Box::new(TestNode {
                    key: 14,
                    parent_k: Some(15),
                    left_k: Some(Box::new(TestNode {
                        key: 12,
                        parent_k: Some(14),
                        left_k: Some(Box::new(TestNode::leaf(9, 12))),
                        right_k: None,
                    })),
                    right_k: None,
                })),
                right_k: None,
            })),
            right_k: None,
        };
        assert_eq!(&expected3, unsafe { tree.root.as_ref() });

        let node = tree.get_raw(&14).unwrap();
        tree.rotate_right(node);
        assert_eq!(&expected2, unsafe { tree.root.as_ref() });

        tree.rotate_right(tree.root);
        assert_eq!(&expected1, unsafe { tree.root.as_ref() });

        tree.rotate_right(tree.root);
        assert_eq!(&expected0, unsafe { tree.root.as_ref() });
    }

    #[test]
    fn inorder_for_each() {
        let mut tree = RedBlackTree::new();
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
        let mut tree = RedBlackTree::new();
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
        let mut tree = RedBlackTree::new();
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
        let mut tree = RedBlackTree::new();
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
        let mut tree = RedBlackTree::new();
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
        let mut tree = RedBlackTree::new();
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
            if !tree.is_empty() {
                assert_red_blackness(unsafe { tree.root.as_ref() });
            }
        }
    }

    #[test]
    fn delete2() {
        let mut tree = RedBlackTree::new();
        assert_eq!(tree.get(&4), None);
        let inserts = [26, 81, 303, 0];
        for i in inserts {
            tree.insert(i, i);
        }

        for it in [26, 81, 303, 0] {
            assert_eq!(tree.delete(&it), Some((it, it)));
            if !tree.is_empty() {
                assert_red_blackness(unsafe { tree.root.as_ref() });
            }

            //println!("{tree:#?}");
        }
    }

    #[test]
    fn delete3() {
        let mut tree = RedBlackTree::new();
        assert_eq!(tree.get(&4), None);
        let inserts = [3836, 3865, 4173, 1635, 4585, 8422, 4412, 2624, 2138, 128];
        for i in inserts {
            tree.insert(i, i);
        }

        for it in inserts {
            //println!("\n= {it} =\n");
            assert_eq!(tree.delete(&it), Some((it, it)));
            //println!("{tree:#?}");
            if !tree.is_empty() {
                assert_red_blackness(unsafe { tree.root.as_ref() });
            }
        }
    }

    mod proptests {
        use std::collections::hash_map::RandomState;

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
                let mut rbt = RedBlackTree::new();
                for v in &inserts {
                    rbt.insert(*v, *v);
                }
                if !rbt.is_empty() {
                    assert_red_blackness(unsafe{rbt.root.as_ref()});
                }

                inserts.shuffle(&mut thread_rng());
                for key in inserts.iter().chain(access.iter()) {
                    assert_eq!(ref_hmap.get_key_value(key), rbt.get(key));
                }
            }

            #[test]
            fn order(
                inserts in proptest::collection::hash_set(0..10000i32, 0..MAP_SIZE),
            ) {
                let mut rbt = RedBlackTree::new();
                for v in &inserts {
                    rbt.insert(*v, *v);
                }

                let mut inserts: Vec<_> = inserts.into_iter().collect();
                inserts.sort();

                let mut items = Vec::with_capacity(rbt.len());
                rbt.inorder_for_each(|k, _| items.push(*k));
                assert_eq!(&items, &inserts);
            }

            #[test]
            fn successor(
                inserts in proptest::collection::hash_set(0..10000i32, 0..MAP_SIZE),
            ) {
                let mut bst = RedBlackTree::new();
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
                let mut bst = RedBlackTree::new();
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
                let mut tree = RedBlackTree::new();
                for v in &inserts {
                    tree.insert(*v, *v);
                }

                let mut inserts: Vec<_> = inserts.into_iter().collect();
                inserts.shuffle(&mut thread_rng());
                for key in inserts.iter().chain(access.iter()) {
                    assert_eq!(ref_hmap.remove_entry(key), tree.delete(key));
                    if !tree.is_empty() {
                        assert_red_blackness(unsafe { tree.root.as_ref() });
                    }
                }
            }

        );
    }
}
