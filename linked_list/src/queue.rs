use core::marker::PhantomData;
use core::{fmt, ptr};

struct Queue<T> {
    head: *mut Node<T>,
    tail: *mut Node<T>,
    len: usize,
    marker: PhantomData<T>,
}

struct Node<T> {
    data: T,
    next: *mut Node<T>,
}

impl<T> Queue<T> {
    pub fn new() -> Self {
        Self {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
            len: 0,
            marker: PhantomData,
        }
    }

    pub fn push(&mut self, val: T) {
        let new = Node {
            data: val,
            next: ptr::null_mut(),
        };
        let new = Box::into_raw(Box::new(new));

        if self.len == 0 {
            self.head = new;
        } else {
            unsafe { (*self.tail).next = new };
        }
        self.tail = new;
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let head = unsafe { Box::from_raw(self.head) };
        let Node { data, next } = *head;
        self.head = next;
        self.len -= 1;

        if self.len == 0 {
            self.tail = ptr::null_mut();
            // self.head must already be null
            assert!(self.head.is_null())
        }

        Some(data)
    }

    pub fn peek(&self) -> Option<&T> {
        if self.len == 0 {
            return None;
        }

        unsafe { Some(&(*self.head).data) }
    }
}

impl<T> fmt::Debug for Queue<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Queue")
            .field("count", &self.len)
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

impl<T> Drop for Queue<T> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut ll = Queue::new();

        ll.push(5);
        println!("{:?}", ll);

        ll.push(6);
        println!("{:?}", ll);

        ll.pop();
        println!("{:?}", ll);

        ll.push(7);
        println!("{:?}", ll);

        ll.pop();
        println!("{:?}", ll);

        ll.pop();
        println!("{:?}", ll);



    }
}
