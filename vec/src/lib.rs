#![allow(dead_code)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc as crate_alloc;

use core::alloc::Layout;
use core::marker::PhantomData;
use core::{fmt, mem, ptr, slice};
use crate_alloc::alloc;

struct Vec2<T> {
    // TODO: use NonNull for covariant T so Vec2<&'static T> could be used for Vec2<&'a T>, *mut T makes Vec2<T> invariant
    buf: *mut T,
    len: usize,
    cap: usize,
    marker: PhantomData<T>,
}

fn covariant<'a, T>(a: Vec2<&'static T>) -> Vec2<&'a T> {
    // a
    todo!()
}

impl<T> fmt::Debug for Vec2<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vec2")
            .field("len", &self.len)
            .field("cap", &self.cap)
            .field("buf", &self.as_slice())
            .finish()
    }
}

impl<T> Drop for Vec2<T> {
    fn drop(&mut self) {
        if self.buf.is_null() {
            return;
        }

        let len = self.len;
        self.len = 0;

        // TODO: add drop guard in case T::drop panics
        for i in 0..len {
            unsafe {
                let ptr = self.buf.add(i);
                ptr::drop_in_place(ptr);
            }
        }

        let layout = Layout::array::<T>(self.cap).unwrap();
        unsafe { alloc::dealloc(self.buf.cast::<u8>(), layout) };
    }
}

impl<T> Vec2<T> {
    // Notes:
    //  * On any allocation error we panic for now
    //    TODO: add try_grow methods
    const INITIAL_CAP: usize = 2;

    pub fn new() -> Self {
        assert!(mem::size_of::<T>() != 0, "we don't (yet) support ZST");
        Self {
            buf: ptr::null_mut(),
            len: 0,
            cap: 0,
            marker: PhantomData,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        let mut s = Self::new();
        s.grow_to(cap);
        s
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.buf.cast_const(), self.len) }
    }

    fn grow_to(&mut self, new_cap: usize) {
        let (buf, layout) = if self.cap == 0 {
            let layout = Layout::array::<T>(new_cap).unwrap();
            let buf = unsafe { alloc::alloc(layout) };
            (buf, layout)
        } else {
            let old_layout = Layout::array::<T>(self.cap).unwrap();
            let new_layout = Layout::array::<T>(new_cap).unwrap();
            let buf =
                unsafe { alloc::realloc(self.buf.cast::<u8>(), old_layout, new_layout.size()) };
            (buf, new_layout)
        };

        if buf.is_null() {
            alloc::handle_alloc_error(layout)
        }

        let buf = buf.cast::<T>();
        self.buf = buf;
        self.cap = new_cap;
    }

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            Self::INITIAL_CAP
        } else {
            // Cannot owerflow because Layout::array constraints the total
            // number of bytes allocated to be less than isize::MAX.
            // Thus at most self.cap == isize::MAX and isize::MAX * 2 == usize::MAX - 1
            self.cap * 2
        };
        self.grow_to(new_cap);
    }

    pub fn push(&mut self, val: T) {
        if self.len == self.cap {
            self.grow()
        }

        assert!(self.len < self.cap);
        let ptr = unsafe { self.buf.add(self.len) };
        unsafe { ptr.write(val) };
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        self.len -= 1; // Want to read at last index, so decrement before reading
        let ptr = unsafe { self.buf.add(self.len) };
        let val = unsafe { ptr.read() };
        Some(val)
    }

    pub fn get(&mut self, index: usize) -> Option<&T> {
        if !self.is_in_bounds(index) {
            return None;
        }

        let ptr = unsafe { self.buf.add(index) };
        unsafe { Some(&*ptr) }
    }

    #[inline(always)]
    fn is_in_bounds(&self, index: usize) -> bool {
        index < self.len
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        if !self.is_in_bounds(index) {
            return None;
        }

        let ptr = unsafe { self.buf.add(index) };
        let val = unsafe { ptr.read() };

        unsafe {
            // shift tail down by 1 position
            self.len -= 1;
            let tail_start = ptr.add(1);
            let count = self.len - index;
            ptr::copy(tail_start, ptr, count)
        }

        Some(val)
    }

    pub fn insert(&mut self, index: usize, val: T) -> Result<(), T> {
        if index > self.len {
            // index == self.len is ok here, it's equivalent to self.push
            return Err(val);
        }

        if index == self.len {
            self.push(val);
            return Ok(());
        }

        if self.len == self.cap {
            self.grow()
        }

        unsafe {
            // shift tail up by 1 position

            // [head] [tail]   [after]
            //        ^-index  ^-self.len
            let tail_start = self.buf.add(index);
            let count = self.len - index;
            ptr::copy(tail_start, tail_start.add(1), count)
            // [head] [empty]  [tail] [after]
            //        ^-index         ^-self.len
        }

        unsafe {
            // write new value to buf[index]
            let ptr = self.buf.add(index);
            ptr.write(val);
        }

        self.len += 1;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut v = Vec2::new();
        v.push(2);
        println!("{:?}", v);
        v.push(3);
        println!("{:?}", v);
        v.push(4);
        println!("{:?}", v);

        v.pop();
        println!("{:?}", v);
        v.pop();
        println!("{:?}", v);
        v.insert(1, 5).unwrap();
        v.insert(1, 6).unwrap();
        println!("{:?}", v);

        v.remove(1);
        println!("{:?}", v);
    }

    #[test]
    fn it_works2() {
        let mut v = Vec2::new();
        v.push(String::from("2"));
        println!("{:?}", v);
        v.push(String::from("3"));
        println!("{:?}", v);
        v.push(String::from("4"));
        println!("{:?}", v);

        v.pop();
        println!("{:?}", v);
        v.pop();
        println!("{:?}", v);
        //v.pop();
        println!("{:?}", v);
    }
}
