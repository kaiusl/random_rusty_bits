use core::marker::PhantomData;
use core::{fmt, ptr};

struct LinkedList<T> {
    head: *mut Node<T>,
    tail: *mut Node<T>,
    count: usize,
    marker: PhantomData<T>,
}

impl<T> fmt::Debug for LinkedList<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkedList")
            .field("count", &self.count)
            .field("items", &DebugNodes { node: self.head })
            .finish()
    }
}

struct DebugNodes<T> {
    node: *mut Node<T>,
}

impl<T> fmt::Debug for DebugNodes<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt = f.debug_list();

        let mut current = self.node;
        while !current.is_null() {
            let data = unsafe { &(*current).data };
            fmt.entry(data);
            current = unsafe { (*current).next };
        }

        fmt.finish()
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut current = self.head;
        self.head = ptr::null_mut();
        self.tail = ptr::null_mut();
        while !current.is_null() {
            let c = unsafe { Box::from_raw(current) };
            let Node { next, .. } = *c;
            current = next;
        }
    }
}

struct Node<T> {
    data: T,
    next: *mut Node<T>,
    prev: *mut Node<T>,
}

impl<T> LinkedList<T> {
    pub fn new() -> Self {
        Self {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
            count: 0,
            marker: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn push_back(&mut self, val: T) {
        let new = Node {
            data: val,
            next: ptr::null_mut(),
            prev: self.tail,
        };

        let new = Box::into_raw(Box::new(new));
        if self.count == 0 {
            self.head = new;
            self.tail = new;
        } else {
            debug_assert!(!self.tail.is_null());
            unsafe { (*self.tail).next = new };
            self.tail = new;
        }

        self.count += 1;
    }

    pub fn push_front(&mut self, val: T) {
        let new = Node {
            data: val,
            next: self.head,
            prev: ptr::null_mut(),
        };
        let new = Box::into_raw(Box::new(new));

        if self.count == 0 {
            self.head = new;
            self.tail = new;
        } else {
            debug_assert!(!self.head.is_null());
            unsafe { (*self.head).prev = new };
            self.head = new;
        }

        self.count += 1;
    }

    pub fn insert(&mut self, index: usize, val: T) {
        match index {
            0 => self.push_front(val),
            i if i == self.count => self.push_back(val),
            _ => {
                let Some(current) = self.get_raw_mut(index) else {
                    panic!()
                };
                let prev = unsafe { (*current).prev };

                let new = Node {
                    data: val,
                    next: current,
                    prev,
                };
                let new = Box::into_raw(Box::new(new));

                unsafe { (*current).prev = new };

                if !prev.is_null() {
                    unsafe { (*prev).next = new }
                }

                self.count += 1;
            }
        }
    }

    unsafe fn remove_raw(&mut self, val: *mut Node<T>) -> T {
        assert!(!val.is_null());

        let val = unsafe { Box::from_raw(val) };
        let Node { data, next, prev } = *val;

        if !next.is_null() {
            unsafe { (*next).prev = prev }
        } else {
            // val was tail
            self.tail = prev;
        }

        if !prev.is_null() {
            unsafe { (*prev).next = next }
        } else {
            // val was head
            self.head = next;
        }
        self.count -= 1;

        if cfg!(debug_assertions) {
            unsafe {
                if !self.head.is_null() {
                    assert!((*self.head).prev.is_null())
                }

                if !self.tail.is_null() {
                    assert!((*self.tail).next.is_null())
                }

                if self.count == 0 {
                    assert!(self.tail.is_null());
                    assert!(self.head.is_null());
                }
            }
        }

        data
    }

    pub fn remove(&mut self, i: usize) -> Option<T> {
        self.get_raw_mut(i)
            .map(|node| unsafe { self.remove_raw(node) })
    }

    pub fn pop_back(&mut self) -> Option<T> {
        if self.tail.is_null() {
            None
        } else {
            Some(unsafe { self.remove_raw(self.tail) })
        }
    }

    pub fn pop_front(&mut self) -> Option<T> {
        if self.head.is_null() {
            None
        } else {
            Some(unsafe { self.remove_raw(self.head) })
        }
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        self.get_raw(i).map(|a| unsafe { &(*a).data })
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.get_raw_mut(i).map(|a| unsafe { &mut (*a).data })
    }

    pub fn front(&self) -> Option<&T> {
        if self.head.is_null() {
            None
        } else {
            unsafe { Some(&(*self.head).data) }
        }
    }

    pub fn front_mut(&mut self) -> Option<&mut T> {
        if self.head.is_null() {
            None
        } else {
            unsafe { Some(&mut (*self.head).data) }
        }
    }

    pub fn back(&self) -> Option<&T> {
        if self.tail.is_null() {
            None
        } else {
            unsafe { Some(&(*self.tail).data) }
        }
    }

    pub fn back_mut(&mut self) -> Option<&mut T> {
        if self.tail.is_null() {
            None
        } else {
            unsafe { Some(&mut (*self.tail).data) }
        }
    }

    fn get_raw_mut(&mut self, index: usize) -> Option<*mut Node<T>> {
        if index >= self.count {
            return None;
        }

        let mut current = self.head;
        for _ in 0..index {
            assert!(!current.is_null());
            current = unsafe { (*current).next };
        }

        Some(current)
    }

    fn get_raw(&self, index: usize) -> Option<*const Node<T>> {
        if index >= self.count {
            return None;
        }

        let mut current = self.head;
        for _ in 0..index {
            assert!(!current.is_null());
            current = unsafe { (*current).next };
        }

        Some(current)
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

        ll.push_back(6);
        println!("{:?}", ll);

        ll.push_front(8);
        println!("{:?}", ll);

        ll.insert(0, 11);
        println!("{:?}", ll);

        ll.remove(0);
        println!("{:?}", ll);

        ll.remove(1);
        println!("{:?}", ll);

        ll.remove(1);
        println!("{:?}", ll);

        ll.push_front(9);
        println!("{:?}", ll);

        ll.remove(0);
        println!("{:?}", ll);

        ll.pop_front();
        println!("{:?}", ll);

        // ll.push_front(9);
        // println!("{:?}", ll);

        // ll.push_front(9);
        // println!("{:?}", ll);

        //let result = add(2, 2);
        // assert_eq!(result, 4);
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
