use core::marker::PhantomData;
use core::ptr::NonNull;
use core::{fmt, ptr};

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
            .field(
                "items",
                &DebugNodes {
                    node: self.head_ptr(),
                },
            )
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
            let data = unsafe { &(*current.as_ptr()).data };
            fmt.entry(data);
            maybe_current = unsafe { (*current.as_ptr()).next };
        }

        fmt.finish()
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        if let Some(HeadTail { head, .. }) = self.head_tail.as_mut() {
            let mut current = *head;

            loop {
                let c = unsafe { Box::from_raw(current.as_ptr()) };
                let Node { next, .. } = *c;
                match next {
                    Some(next) => current = next,
                    None => break,
                }
            }
        }
    }
}

struct Node<T> {
    data: T,
    next: Option<NonNull<Node<T>>>,
    prev: Option<NonNull<Node<T>>>,
}

impl<T> LinkedList<T> {
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

    fn set_tail(&mut self, tail: NonNull<Node<T>>) {
        self.head_tail.as_mut().unwrap().tail = tail;
    }

    fn set_head(&mut self, head: NonNull<Node<T>>) {
        self.head_tail.as_mut().unwrap().head = head
    }

    pub fn push_back(&mut self, val: T) {
        let new = Node {
            data: val,
            next: None,
            prev: self.tail_ptr(),
        };

        let new = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(new))) };
        match &mut self.head_tail {
            Some(HeadTail { tail, .. }) => {
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
        let new = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(new))) };
        match &mut self.head_tail {
            Some(HeadTail { head, .. }) => {
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

    pub fn insert(&mut self, index: usize, val: T) {
        match index {
            0 => self.push_front(val),
            i if i == self.count => self.push_back(val),
            _ => {
                let Some(current) = self.get_raw(index) else {
                    panic!()
                };
                let prev = unsafe { (*current.as_ptr()).prev.unwrap() };

                let new = Node {
                    data: val,
                    next: Some(current),
                    prev: Some(prev),
                };
                let new = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(new))) };

                unsafe { (*current.as_ptr()).prev = Some(new) };
                unsafe { (*prev.as_ptr()).next = Some(new) }

                self.count += 1;
            }
        }
    }

    unsafe fn remove_raw(&mut self, val: NonNull<Node<T>>) -> T {
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
                unsafe {
                    (*prev.as_ptr()).next = Some(next);
                    (*next.as_ptr()).prev = Some(prev);
                }
            }
            (Some(prev), None) => {
                // tail
                unsafe { (*prev.as_ptr()).next = None };
                self.set_tail(prev);
            }
            (None, Some(next)) => {
                // head
                unsafe { (*next.as_ptr()).prev = None };
                self.set_head(next);
            }
        }

        self.count -= 1;
        data
    }

    pub fn remove(&mut self, i: usize) -> Option<T> {
        self.get_raw(i).map(|node| unsafe { self.remove_raw(node) })
    }

    pub fn pop_back(&mut self) -> Option<T> {
        match self.head_tail.as_mut() {
            Some(HeadTail { tail, .. }) => {
                let tail = *tail;
                Some(unsafe { self.remove_raw(tail) })
            }
            None => None,
        }
    }

    pub fn pop_front(&mut self) -> Option<T> {
        match self.head_tail.as_mut() {
            Some(HeadTail { head, .. }) => {
                let head = *head;
                Some(unsafe { self.remove_raw(head) })
            }
            None => None,
        }
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        self.get_raw(i).map(|a| unsafe { &(*a.as_ptr()).data })
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.get_raw(i).map(|a| unsafe { &mut (*a.as_ptr()).data })
    }

    pub fn front(&self) -> Option<&T> {
        self.head_tail
            .as_ref()
            .map(|ht| unsafe { &(*ht.head.as_ptr()).data })
    }

    pub fn front_mut(&mut self) -> Option<&mut T> {
        self.head_tail
            .as_ref()
            .map(|ht| unsafe { &mut (*ht.head.as_ptr()).data })
    }

    pub fn back(&self) -> Option<&T> {
        self.head_tail
            .as_ref()
            .map(|ht| unsafe { &(*ht.tail.as_ptr()).data })
    }

    pub fn back_mut(&mut self) -> Option<&mut T> {
        self.head_tail
            .as_ref()
            .map(|ht| unsafe { &mut (*ht.tail.as_ptr()).data })
    }

    fn get_raw(&self, index: usize) -> Option<NonNull<Node<T>>> {
        if index >= self.count {
            return None;
        }

        let mut current = self.head_ptr().unwrap();
        for _ in 0..index {
            current = unsafe { (*current.as_ptr()).next.unwrap() };
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
