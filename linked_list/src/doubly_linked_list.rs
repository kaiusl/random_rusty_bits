use core::marker::PhantomData;
use core::ptr::NonNull;
use core::{fmt, ptr};

use self::iter::{Iter, IterMut};

struct LinkedList<T> {
    // Head and tail can only be None both at once (when count == 0).
    // If count == 1 both point to the same item.
    head_tail: Option<HeadTail<T>>,
    count: usize,
    marker: PhantomData<T>,
}

struct HeadTail<T> {
    head: NonNull<Node<T>>,
    tail: NonNull<Node<T>>,
}

impl<T> fmt::Debug for LinkedList<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkedList")
            .field("count", &self.count)
            .field("items", &self.iter())
            .finish()
    }
}

struct DebugNodes<T> {
    node: Option<NonNull<Node<T>>>,
}

impl<T> fmt::Debug for DebugNodes<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt = f.debug_list();

        let mut maybe_current = self.node;
        while let Some(current) = maybe_current {
            // SAFETY: all node pointers are valid to deref (see safety doc on top of this impl block)
            let data = unsafe { &(*current.as_ptr()).data };
            fmt.entry(data);
            maybe_current = unsafe { (*current.as_ptr()).next };
        }

        fmt.finish()
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        /// Guard in case `T::drop` panics.
        ///
        /// We try to clean up as much as possible after the panic, eg try to
        /// drop the remaining items.
        struct Guard<U>(Option<NonNull<Node<U>>>);

        impl<U> Guard<U> {
            fn drop_items(&mut self) {
                // Take self.0 so we cannot try to drop the same U again.
                while let Some(current) = self.0.take() {
                    // shadow current so it cannot be used again as it's not valid to be used again
                    // SAFETY: All pointer are derived from valid Box
                    let mut current = unsafe { Box::from_raw(current.as_ptr()) };
                    // data needs to be dropped after self.0 = next
                    // because this way we can try to drop the remaining items
                    // after U::drop panics and clean up as much as possible.
                    //
                    // Otherwise since we self.0.take() we would leak all
                    // remaining items after the panic as self.0 is None.
                    self.0 = current.next.take();
                    drop(current);
                }
            }
        }

        impl<U> Drop for Guard<U> {
            fn drop(&mut self) {
                self.drop_items()
            }
        }

        self.count = 0;
        let mut guard = Guard(self.head_tail.take().map(|a| a.head));
        guard.drop_items()
    }
}

struct Node<T> {
    data: T,
    next: Option<NonNull<Node<T>>>,
    prev: Option<NonNull<Node<T>>>,
}

impl<T> LinkedList<T> {
    // SAFETY INVARIANTS:
    //   * All node pointers (`NonNull<Node<T>>`) which are reachable from head/tail pointers are:
    //     - valid to dereference, they are never set to `NonNull::dangling` and are aligned
    //       since they are created from a real `Box`
    //     - stable, we never move any of the allocated nodes
    //     - alive for the lifetime of self as they are deallocated only in Self::drop

    pub fn new() -> Self {
        Self {
            head_tail: None,
            count: 0,
            marker: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    fn tail_ptr(&self) -> Option<NonNull<Node<T>>> {
        self.head_tail.as_ref().map(|a| a.tail)
    }

    fn head_ptr(&self) -> Option<NonNull<Node<T>>> {
        self.head_tail.as_ref().map(|a| a.head)
    }

    /// Set tail pointer. Assumes that self.head_tail is Some
    fn set_tail(&mut self, tail: NonNull<Node<T>>) {
        self.head_tail.as_mut().unwrap().tail = tail;
    }

    /// Set head pointer. Assumes that self.head_tail is Some
    fn set_head(&mut self, head: NonNull<Node<T>>) {
        self.head_tail.as_mut().unwrap().head = head
    }

    pub fn push_back(&mut self, val: T) {
        let new = Node {
            data: val,
            next: None,
            prev: self.tail_ptr(),
        };

        let new = non_null_from_box(Box::new(new));
        match &mut self.head_tail {
            Some(HeadTail { tail, .. }) => {
                // SAFETY:
                //  * &mut self invalidates any previously out given references
                //    (hence no-one else can have reference to `tail`)
                //  * tail must be valid to deref (see safety doc on top of this impl block)
                unsafe { (*tail.as_ptr()).next = Some(new) };
                *tail = new;
            }
            None => {
                debug_assert_eq!(self.count, 0);
                self.head_tail = Some(HeadTail {
                    head: new,
                    tail: new,
                });
            }
        }

        self.count += 1;
    }

    pub fn push_front(&mut self, val: T) {
        let new = Node {
            data: val,
            next: self.head_ptr(),
            prev: None,
        };
        let new = non_null_from_box(Box::new(new));

        match &mut self.head_tail {
            Some(HeadTail { head, .. }) => {
                // SAFETY:
                //  * &mut self invalidates any previously out given references
                //    (hence no-one else can have reference to `head`)
                //  * head must be valid to deref (see safety doc on top of this impl block)
                unsafe { (*head.as_ptr()).prev = Some(new) };
                *head = new;
            }
            None => {
                debug_assert_eq!(self.count, 0);
                self.head_tail = Some(HeadTail {
                    head: new,
                    tail: new,
                });
            }
        }

        self.count += 1;
    }

    pub fn insert(&mut self, index: usize, val: T) -> Result<(), T> {
        match index {
            0 => self.push_front(val),
            i if i == self.count => self.push_back(val),
            _ => {
                let Some(current) = self.get_node(index) else {
                    return Err(val);
                };
                // SAFETY:
                //  * &mut self invalidates any previously out given references
                //    (hence no-one else can have reference to `current` and `prev`)
                //  * all node pointers are valid to deref (see safety doc on top of this impl block)
                let prev = unsafe {
                    (*current.as_ptr()).prev.unwrap_or_else(|| {
                        panic!("expected a node at `index = {index} > 0` to have a previous node")
                    })
                };

                let new = Node {
                    data: val,
                    next: Some(current),
                    prev: Some(prev),
                };
                let new = non_null_from_box(Box::new(new));

                // SAFETY:
                //  * &mut self invalidates any previously out given references
                //    (hence no-one else can have reference to `current` and `prev`)
                //  * all node pointers are valid to deref (see safety doc on top of this impl block)
                unsafe { (*current.as_ptr()).prev = Some(new) };
                unsafe { (*prev.as_ptr()).next = Some(new) }

                self.count += 1;
            }
        }

        Ok(())
    }

    /// # SAFETY
    ///
    /// * `val` must be a valid pointer which is in our list
    unsafe fn remove_node(&mut self, val: NonNull<Node<T>>) -> T {
        // SAFETY: all nodes are constructed from Box::into_raw
        let val = unsafe { Box::from_raw(val.as_ptr()) };
        let Node { data, next, prev } = *val;
        match (prev, next) {
            (None, None) => {
                // only item
                debug_assert_eq!(self.count, 1);
                self.head_tail = None;
            }
            (Some(prev), Some(next)) => {
                // middle
                // SAFETY:
                //  * &mut self invalidates any previously out given references
                //    (hence no-one else can have reference to `next` and `prev`)
                //  * all node pointers are valid to deref (see safety doc on top of this impl block)
                unsafe {
                    (*prev.as_ptr()).next = Some(next);
                    (*next.as_ptr()).prev = Some(prev);
                }
            }
            (Some(prev), None) => {
                // tail
                // SAFETY: see previous branch
                unsafe { (*prev.as_ptr()).next = None };
                self.set_tail(prev);
            }
            (None, Some(next)) => {
                // head
                // SAFETY: see previous branch
                unsafe { (*next.as_ptr()).prev = None };
                self.set_head(next);
            }
        }

        self.count -= 1;
        data
    }

    pub fn remove(&mut self, i: usize) -> Option<T> {
        // SAFETY: get_node return a valid pointer from our list or None
        self.get_node(i).map(|node| unsafe { self.remove_node(node) })
    }

    pub fn pop_back(&mut self) -> Option<T> {
        match self.head_tail.as_mut() {
            Some(HeadTail { tail, .. }) => {
                let tail = *tail;
                // SAFETY: tail is a valid pointer to deref if self.head_tail is Some
                Some(unsafe { self.remove_node(tail) })
            }
            None => None,
        }
    }

    pub fn pop_front(&mut self) -> Option<T> {
        match self.head_tail.as_mut() {
            Some(HeadTail { head, .. }) => {
                let head = *head;
                // SAFETY: head is a valid pointer to deref if self.head_tail is Some
                Some(unsafe { self.remove_node(head) })
            }
            None => None,
        }
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        // SAFETY:
        //  * returned reference is bound to the borrow of self
        //    since we own the data, it must be alive
        //  * all node pointers are valid to deref (see safety doc on top of this impl block)
        self.get_node(i).map(|a| unsafe { &(*a.as_ptr()).data })
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        // SAFETY:
        //  * returned reference is bound to the borrow of self
        //    since we own the data, it must be alive
        //  * Any previously returned references are invalidated by taking &mut self
        //  * all node pointers are valid to deref (see safety doc on top of this impl block)
        self.get_node(i).map(|a| unsafe { &mut (*a.as_ptr()).data })
    }

    pub fn front(&self) -> Option<&T> {
        // SAFETY:
        //  * returned reference is bound to the borrow of self
        //    since we own the data, it must be alive
        //  * self.head_tail contains valid pointers to deref if is is Some
        self.head_tail
            .as_ref()
            .map(|ht| unsafe { &(*ht.head.as_ptr()).data })
    }

    pub fn front_mut(&mut self) -> Option<&mut T> {
        // SAFETY:
        //  * returned reference is bound to the borrow of self
        //    since we own the data, it must be alive
        //  * Any previously returned references are invalidated by taking &mut self
        //  * self.head_tail contains valid pointers to deref if is is Some
        self.head_tail
            .as_ref()
            .map(|ht| unsafe { &mut (*ht.head.as_ptr()).data })
    }

    pub fn back(&self) -> Option<&T> {
        // SAFETY: see self.front
        self.head_tail
            .as_ref()
            .map(|ht| unsafe { &(*ht.tail.as_ptr()).data })
    }

    pub fn back_mut(&mut self) -> Option<&mut T> {
        // SAFETY: see self.front_mut
        self.head_tail
            .as_ref()
            .map(|ht| unsafe { &mut (*ht.tail.as_ptr()).data })
    }

    fn get_node(&self, index: usize) -> Option<NonNull<Node<T>>> {
        if index >= self.count {
            return None;
        }

        // Head must be Some if index < self.count (0 < index < 0 cannot be true)
        let mut current = self.head_ptr().unwrap();
        for _ in 0..index {
            // next must be Some since index < self.count and loop will terminate
            // after we set current = tail
            // SAFETY: all node pointers are valid to deref (see safety doc on top of this impl block)
            current = unsafe { (*current.as_ptr()).next.unwrap() };
        }

        Some(current)
    }

    fn iter(&self) -> Iter<'_, T> {
        Iter::new(self)
    }

    fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut::new(self)
    }
}

fn non_null_from_box<T>(val: Box<T>) -> NonNull<T> {
    // SAFETY: Box::into_raw returns properly aligned and non-null pointer
    unsafe { NonNull::new_unchecked(Box::into_raw(val)) }
}

mod iter {
    use super::*;

    pub struct Iter<'a, T> {
        node: Option<NonNull<Node<T>>>,
        marker: PhantomData<&'a T>,
    }

    impl<'a, T> Iter<'a, T> {
        pub(super) fn new(list: &'a LinkedList<T>) -> Self {
            // SAFETY:
            //  * the returned item's lifetime is bound to the borrow of list,
            //   as the list owns the items they must remain live for 'a
            //  * invariants of `LinkedList` hold here too, see the comment on top of LinkedList impl block
            Self {
                node: list.head_ptr(),
                marker: PhantomData,
            }
        }
    }

    impl<'a, T> Iterator for Iter<'a, T> {
        type Item = &'a T;

        fn next(&mut self) -> Option<Self::Item> {
            match self.node {
                Some(ptr) => {
                    // SAFETY:
                    //  * all node pointer are valid to dereference because they are from `LinkedList`
                    //   (see the safety comment of top of `impl LinkedList` block)
                    let data = unsafe { &(*ptr.as_ptr()).data };
                    self.node = unsafe { (*ptr.as_ptr()).next };

                    Some(data)
                }
                None => None,
            }
        }
    }

    impl<T> Clone for Iter<'_, T> {
        fn clone(&self) -> Self {
            Self {
                node: self.node,
                marker: self.marker,
            }
        }
    }

    impl<T> fmt::Debug for Iter<'_, T>
    where
        T: fmt::Debug,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_list().entries(self.clone()).finish()
        }
    }

    pub struct IterMut<'a, T> {
        node: Option<NonNull<Node<T>>>,
        marker: PhantomData<&'a mut T>,
    }

    impl<'a, T> IterMut<'a, T> {
        pub(super) fn new(list: &'a mut LinkedList<T>) -> Self {
            // SAFETY:
            //  * the returned item's lifetime is bound to the borrow of list,
            //   as the list owns the items they must remain live for 'a
            //  * invariants of `LinkedList` hold here too, see the comment on top of LinkedList impl block
            //  * taking `LinkedList` by &mut will invalidate all previously returned
            //    references by the list since they are all bound to borrow of list
            Self {
                node: list.head_ptr(),
                marker: PhantomData,
            }
        }
    }

    impl<'a, T> Iterator for IterMut<'a, T> {
        type Item = &'a mut T;

        fn next(&mut self) -> Option<Self::Item> {
            match self.node {
                Some(ptr) => {
                    // SAFETY:
                    //  * all node pointer are valid to dereference because they are from `LinkedList`
                    //   (see the safety comment of top of `impl LinkedList` block)
                    //  * all nodes in `LinkedList` point to different nodes,
                    //    thus we cannot return multiple unique references to same data
                    let ptr = ptr.as_ptr();
                    let data = unsafe { &mut (*ptr).data };
                    self.node = unsafe { (*ptr).next };

                    Some(data)
                }
                None => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut ll = LinkedList::new();

        ll.push_back(5);
        println!("{:?}", ll);

        // ll.push_back(6);
        // println!("{:?}", ll);

        // ll.push_front(8);
        // println!("{:?}", ll);

        // ll.insert(0, 11);
        // println!("{:?}", ll);

        // ll.remove(0);
        // println!("{:?}", ll);

        // ll.remove(1);
        // println!("{:?}", ll);

        // ll.remove(1);
        // println!("{:?}", ll);

        // ll.push_front(9);
        // println!("{:?}", ll);

        // ll.remove(0);
        // println!("{:?}", ll);

        // ll.pop_front();
        // println!("{:?}", ll);

        println!("{:?}", ll.get(0));

        // ll.push_front(9);
        // println!("{:?}", ll);

        // ll.push_front(9);
        // println!("{:?}", ll);

        //let result = add(2, 2);
        // assert_eq!(result, 4);
    }

    #[test]
    fn iters() {
        let mut ll = LinkedList::new();

        ll.push_back(5);
        ll.push_back(6);
        ll.push_front(8);
        ll.insert(0, 11).unwrap();
        ll.push_front(9);

        let vals: Vec<_> = ll.iter().collect();
        assert_eq!(vals, [&9, &11, &8, &5, &6]);

        let vals: Vec<_> = ll.iter_mut().collect();
        assert_eq!(vals, [&9, &11, &8, &5, &6]);
    }

    #[test]
    fn test_basic_front() {
        let mut list = LinkedList::new();

        // Try to break an empty list
        assert_eq!(list.len(), 0);
        assert_eq!(list.pop_front(), None);
        assert_eq!(list.len(), 0);

        // Try to break a one item list
        list.push_front(10);
        assert_eq!(list.len(), 1);
        assert_eq!(list.pop_front(), Some(10));
        assert_eq!(list.len(), 0);
        assert_eq!(list.pop_front(), None);
        assert_eq!(list.len(), 0);

        // Mess around
        list.push_front(10);
        assert_eq!(list.len(), 1);
        list.push_front(20);
        assert_eq!(list.len(), 2);
        list.push_front(30);
        assert_eq!(list.len(), 3);
        assert_eq!(list.pop_front(), Some(30));
        assert_eq!(list.len(), 2);
        list.push_front(40);
        assert_eq!(list.len(), 3);
        assert_eq!(list.pop_front(), Some(40));
        assert_eq!(list.len(), 2);
        assert_eq!(list.pop_front(), Some(20));
        assert_eq!(list.len(), 1);
        assert_eq!(list.pop_front(), Some(10));
        assert_eq!(list.len(), 0);
        assert_eq!(list.pop_front(), None);
        assert_eq!(list.len(), 0);
        assert_eq!(list.pop_front(), None);
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_basic() {
        let mut m = LinkedList::new();
        assert_eq!(m.pop_front(), None);
        assert_eq!(m.pop_back(), None);
        assert_eq!(m.pop_front(), None);
        m.push_front(1);
        assert_eq!(m.pop_front(), Some(1));
        m.push_back(2);
        m.push_back(3);
        assert_eq!(m.len(), 2);
        assert_eq!(m.pop_front(), Some(2));
        assert_eq!(m.pop_front(), Some(3));
        assert_eq!(m.len(), 0);
        assert_eq!(m.pop_front(), None);
        m.push_back(1);
        m.push_back(3);
        m.push_back(5);
        m.push_back(7);
        assert_eq!(m.pop_front(), Some(1));

        let mut n = LinkedList::new();
        n.push_front(2);
        n.push_front(3);
        {
            assert_eq!(n.front().unwrap(), &3);
            let x = n.front_mut().unwrap();
            assert_eq!(*x, 3);
            *x = 0;
        }
        {
            assert_eq!(n.back().unwrap(), &2);
            let y = n.back_mut().unwrap();
            assert_eq!(*y, 2);
            *y = 1;
        }
        assert_eq!(n.pop_front(), Some(0));
        assert_eq!(n.pop_front(), Some(1));
    }
}
