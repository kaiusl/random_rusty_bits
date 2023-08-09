extern crate alloc as crate_alloc;

use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::{fmt, mem, ptr, slice};
use crate_alloc::alloc;

struct VecDeque2<T> {
    buf: NonNull<T>,
    head: usize,
    len: usize,
    cap: usize,
    marker: PhantomData<T>,
}

fn covariant<'a, T>(a: VecDeque2<&'static T>) -> VecDeque2<&'a T> {
    a
}

impl<T> fmt::Debug for VecDeque2<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VecDeque2")
            .field("len", &self.len)
            .field("cap", &self.cap)
            .field("head", &self.head)
            .field("buf", &self.as_slices())
            .finish()
    }
}

impl<T> Drop for VecDeque2<T> {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }

        while self.pop_back().is_some() {}

        let old_layout = self.current_layout();
        let old_buf = mem::replace(&mut self.buf, NonNull::dangling());
        self.cap = 0;
        self.head = 0;

        unsafe { alloc::dealloc(old_buf.as_ptr().cast::<u8>(), old_layout) };
    }
}

impl<T> VecDeque2<T> {
    // Notes:
    //  * On any allocation error we panic for now
    //    TODO: add try_grow methods
    const INITIAL_CAP: usize = 2;

    pub fn new() -> Self {
        assert!(mem::size_of::<T>() != 0, "we don't (yet) support ZST");
        Self {
            // SAFETY: self.buf is never touched before actually initializing it
            buf: NonNull::dangling(),
            head: 0,
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

    // right and left counts assuming that self is wrapped around
    fn right_left_counts(&self) -> (usize, usize) {
        debug_assert!(self.is_wrapped());
        // [left]  [empty]  [right] [after buf]
        // ^- 0             ^- head ^- cap
        //      ^- left_count-1   ^- head+right_count-1
        let right_count = self.cap - self.head;
        let left_count = self.len - right_count;
        (right_count, left_count)
    }

    fn is_wrapped(&self) -> bool {
        self.head + self.len > self.cap
    }

    pub fn as_slices(&self) -> (&[T], &[T]) {
        if self.cap == 0 {
            // self.buf is dangling as we haven't initialized it
            return (&[], &[]);
        }
        if self.is_wrapped() {
            // [left]  [empty]  [right]
            // ^- 0             ^- head
            //      ^- left_count-1   ^- head+right_count-1
            let (right_count, left_count) = self.right_left_counts();

            let right_start = unsafe { self.buf.as_ptr().add(self.head).cast_const() };
            let right = unsafe { slice::from_raw_parts(right_start, right_count) };
            let left = unsafe { slice::from_raw_parts(self.buf.as_ptr(), left_count) };
            (right, left)
        } else {
            let right = unsafe {
                slice::from_raw_parts(self.buf.as_ptr().add(self.head).cast_const(), self.len)
            };
            (right, &[])
        }
    }

    #[inline]
    fn current_layout(&self) -> Layout {
        // This cannot return Err variant as we have already checked it
        Layout::array::<T>(self.cap).unwrap()
    }

    fn grow_to(&mut self, new_cap: usize) {
        if new_cap <= self.cap {
            return;
        }

        let layout = Layout::array::<T>(new_cap).unwrap();
        let buf = unsafe { alloc::alloc(layout) };

        if buf.is_null() {
            alloc::handle_alloc_error(layout)
        } else {
            let buf = buf.cast::<T>();
            if self.is_wrapped() {
                let (right_count, left_count) = self.right_left_counts();
                // [left]  [empty]  [right]
                // ^- 0             ^- head
                //      ^- left_count-1   ^- head+right_count-1
                //
                // Result:
                //  [right]  [left]  [empty]
                //  ^- 0     ^- right_count
                //                ^- right_count+left_count-1
                unsafe {
                    ptr::copy_nonoverlapping(self.buf.as_ptr().add(self.head), buf, right_count)
                };
                unsafe {
                    ptr::copy_nonoverlapping(self.buf.as_ptr(), buf.add(right_count), left_count)
                };
            } else if self.len != 0 {
                // [empty] [filled] [empty]
                //         ^- head
                //                ^- head+len-1
                //
                // Result:
                //   [filled] [empty]
                //   ^- head=0
                //          ^- len-1
                unsafe { ptr::copy_nonoverlapping(self.buf.as_ptr().add(self.head), buf, self.len) }
            }

            let old_layout = self.current_layout();
            let old_buf = mem::replace(&mut self.buf, unsafe {
                NonNull::new_unchecked(buf.cast::<T>())
            });
            let olc_cap = self.cap;
            self.cap = new_cap;
            self.head = 0;

            if olc_cap != 0 {
                unsafe { alloc::dealloc(old_buf.as_ptr().cast::<u8>(), old_layout) };
            }
        }
    }

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            Self::INITIAL_CAP
        } else {
            // Cannot overflow because Layout::array constraints the total
            // number of bytes allocated to be less than isize::MAX.
            // Thus at most self.cap == isize::MAX and isize::MAX * 2 == usize::MAX - 1
            self.cap * 2
        };
        self.grow_to(new_cap);
    }

    pub fn push_back(&mut self, val: T) {
        if self.len == self.cap {
            self.grow()
        }

        debug_assert!(self.len < self.cap);
        let index = (self.head + self.len) % self.cap;

        let ptr = unsafe { self.buf.as_ptr().add(index) };
        unsafe { ptr.write(val) };
        self.len += 1;
    }

    pub fn push_front(&mut self, val: T) {
        if self.len == self.cap {
            self.grow()
        }

        debug_assert!(self.len < self.cap);
        let index = if self.head == 0 {
            self.cap - 1
        } else {
            self.head - 1
        };
        let ptr = unsafe { self.buf.as_ptr().add(index) };
        unsafe { ptr.write(val) };
        self.len += 1;
        self.head = index;
    }

    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        self.len -= 1; // Want to read at last index, so decrement before reading
        let ptr = unsafe { self.buf.as_ptr().add(self.head) };
        let val = unsafe { ptr.read() };
        // if new len == 0, self.head can be any index into our buffer
        self.head = if self.head == self.cap - 1 {
            // head was last element in out buffer, wrap around the buffer
            0
        } else {
            self.head + 1
        };
        Some(val)
    }

    /// Index of last item. Assumes that self is not empty.
    ///
    /// If self is empty, this function returns an meaningless number or panics.
    #[inline]
    fn tail_index(&mut self) -> usize {
        self.get_real_index(self.len - 1)
    }

    /// The actual index of an index'th element. Assumes that self is not empty
    /// and that the index is in bounds.
    ///
    /// If self is empty, the returned value has no real meaning and is the index
    /// is out of bounds it will wrap around the buffer potentially resulting in
    /// a index to random element or even to uninitialized element.
    #[inline]
    fn get_real_index(&self, index: usize) -> usize {
        debug_assert!(!self.is_empty());
        debug_assert!(self.is_in_bounds(index));
        (self.head + index) % self.cap
    }

    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let index = self.tail_index();
        self.len -= 1;
        let ptr = unsafe { self.buf.as_ptr().add(index) };
        let val = unsafe { ptr.read() };
        Some(val)
    }

    pub fn get(&mut self, index: usize) -> Option<&T> {
        if !self.is_in_bounds(index) {
            return None;
        }

        let index = self.get_real_index(index);
        let ptr = unsafe { self.buf.as_ptr().add(index) };
        unsafe { Some(&*ptr) }
    }

    #[inline(always)]
    fn is_in_bounds(&self, index: usize) -> bool {
        index < self.len
    }

    // pub fn remove(&mut self, index: usize) -> Option<T> {
    //     if !self.is_in_bounds(index) {
    //         return None;
    //     }

    //     let ptr = unsafe { self.buf.as_ptr().add(index) };
    //     let val = unsafe { ptr.read() };

    //     unsafe {
    //         // shift tail down by 1 position
    //         self.len -= 1;
    //         let tail_start = ptr.add(1);
    //         let count = self.len - index;
    //         ptr::copy(tail_start, ptr, count)
    //     }

    //     Some(val)
    // }

    // pub fn insert(&mut self, index: usize, val: T) -> Result<(), T> {
    //     if index > self.len {
    //         // index == self.len is ok here, it's equivalent to self.push
    //         return Err(val);
    //     }

    //     if index == self.len {
    //         self.push(val);
    //         return Ok(());
    //     }

    //     if self.len == self.cap {
    //         self.grow()
    //     }

    //     unsafe {
    //         // shift tail up by 1 position

    //         // [head] [tail]   [after]
    //         //        ^-index  ^-self.len
    //         let tail_start = self.buf.as_ptr().add(index);
    //         let count = self.len - index;
    //         ptr::copy(tail_start, tail_start.add(1), count)
    //         // [head] [empty]  [tail] [after]
    //         //        ^-index         ^-self.len
    //     }

    //     unsafe {
    //         // write new value to buf[index]
    //         let ptr = self.buf.as_ptr().add(index);
    //         ptr.write(val);
    //     }

    //     self.len += 1;

    //     Ok(())
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push() {
        let mut v = VecDeque2::new();
        v.push_back(2);
        println!("{:?}", v);
        v.push_front(3);
        println!("{:?}", v);
        v.push_back(4);
        println!("{:?}", v);
        v.push_front(5);
        println!("{:?}", v);
        v.push_front(6);
        println!("{:?}", v);
        v.push_front(7);
        println!("{:?}", v);
        v.push_back(8);
        println!("{:?}", v);
    }

    #[test]
    fn pop() {
        let mut v = VecDeque2::new();
        v.push_back(2);
        v.push_front(3);
        println!("start={:?}", v);
        v.pop_back();
        println!("{:?}", v);
        v.pop_back();
        println!("{:?}", v);

        v.push_back(2);
        v.push_front(3);
        v.push_back(4);
        v.push_front(5);
        v.push_front(6);
        v.push_front(7);
        println!("start={:?}", v);
        v.pop_back();
        println!("{:?}", v);
        v.pop_front();
        println!("{:?}", v);
        v.pop_front();
        println!("{:?}", v);
        v.pop_front();
        println!("{:?}", v);
        v.pop_back();
        println!("{:?}", v);
    }

    #[test]
    fn get() {
        let mut v = VecDeque2::new();
        v.push_back(2);
        v.push_front(3);
        v.push_back(4);
        v.push_front(5);
        v.push_front(6);
        v.push_front(7);
        v.push_back(8);

        assert_eq!(v.get(0), Some(&7));
        assert_eq!(v.get(1), Some(&6));
        assert_eq!(v.get(2), Some(&5));
        assert_eq!(v.get(3), Some(&3));
        assert_eq!(v.get(4), Some(&2));
        assert_eq!(v.get(5), Some(&4));
        assert_eq!(v.get(6), Some(&8));
        assert_eq!(v.get(7), None);
    }

    // #[test]
    // fn it_works2() {
    //     let mut v = Vec2::new();
    //     v.push(String::from("2"));
    //     println!("{:?}", v);
    //     v.push(String::from("3"));
    //     println!("{:?}", v);
    //     v.push(String::from("4"));
    //     println!("{:?}", v);

    //     v.pop();
    //     println!("{:?}", v);
    //     v.pop();
    //     println!("{:?}", v);
    //     //v.pop();
    //     println!("{:?}", v);
    // }
}
