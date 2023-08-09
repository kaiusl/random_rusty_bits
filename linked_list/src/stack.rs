use core::marker::PhantomData;
use core::{fmt, ptr};

struct Stack<T> {
    head: *mut Node<T>,
    len: usize,
    marker: PhantomData<T>,
}

struct Node<T> {
    data: T,
    prev: *mut Node<T>,
}

impl<T> Stack<T> {
    pub fn new() -> Self {
        Self {
            head: ptr::null_mut(),
            len: 0,
            marker: PhantomData,
        }
    }

    pub fn push(&mut self, val: T) {
        let new = Node {
            data: val,
            prev: self.head,
        };
        let new = Box::into_raw(Box::new(new));

        self.head = new;
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let head = unsafe { Box::from_raw(self.head) };
        let Node { data, prev } = *head;
        self.head = prev;
        self.len -= 1;

        Some(data)
    }

    pub fn peek(&self) -> Option<&T> {
        if self.len == 0 {
            return None;
        }

        unsafe { Some(&(*self.head).data) }
    }
}

impl<T> fmt::Debug for Stack<T>
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
            current = unsafe { (*current).prev };
        }

        fmt.finish()
    }
}

impl<T> Drop for Stack<T> {
    fn drop(&mut self) {
        let mut current = self.head;
        self.head = ptr::null_mut();
        while !current.is_null() {
            let c = unsafe { Box::from_raw(current) };
            let Node { prev, .. } = *c;
            current = prev;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut ll = Stack::new();

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
