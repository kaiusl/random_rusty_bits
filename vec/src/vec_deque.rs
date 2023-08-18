extern crate alloc as crate_alloc;

use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::{fmt, mem, ptr, slice};

use crate_alloc::alloc;

struct VecDeque2<T> {
    // INVARIANTS:
    //  * `len <= cap` and `head < cap` or if `cap == 0` then `head == len == cap == 0`
    //  * `len` contiguous elements are initialized in `buf` starting from `head`
    //    (they may wrap around the `buf`) (is there a better way to word this???)
    //  * `buf` is valid pointer to contiguous memory to store `cap` `T`s
    //    (`buf` can only be `NonNull::dangling` if `cap == len == 0`)
    buf: NonNull<T>,
    head: usize,
    len: usize,
    cap: usize,
    marker: PhantomData<T>,
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

        /// Drop guard in case T::drop panics.
        ///
        /// In the case on unwinding we try to drop the remaining items.
        /// If that succeeds we deallocate our buffer and the caller could catch the unwinding,
        /// if not we abort due to double panic.
        struct Guard<'a, U>(&'a mut VecDeque2<U>);

        impl<'a, U> Drop for Guard<'a, U> {
            fn drop(&mut self) {
                while self.0.pop_back().is_some() {}

                assert_eq!(self.0.len, 0);

                // We haven't yet updated self.buf and self.cap
                let layout = self.0.current_layout();
                self.0.cap = 0;
                self.0.head = 0;
                let buf = mem::replace(&mut self.0.buf, NonNull::dangling())
                    .as_ptr()
                    .cast::<u8>();

                // SAFETY:
                //  * we allocate only with Global allocator (we don't support custom allocators)
                unsafe { alloc::dealloc(buf, layout) };
            }
        }

        let g = Guard(self);
        while g.0.pop_back().is_some() {}
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
            // SAFETY: self.buf is never touched before actually allocating it
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

    /// Right and left counts assuming that self is wrapped around.
    ///
    /// It is safe to call while self is not wrapped but the counts are wrong.
    fn right_left_counts(&self) -> (usize, usize) {
        debug_assert!(self.is_wrapped());
        // [left]  [empty]  [right] [after buf]
        // ^- 0             ^- head ^- cap
        //      ^- left_count-1   ^- head+right_count-1
        //
        // An example:
        // [uninit, uninit, uninit, uninit, 0, 1, 2, 3]
        //   cap = 8, len = 4, head = 4
        //   rc = 8 - 4 = 4, lc = 4 - 4 = 0
        // [5, uninit, uninit, uninit, 0, 1, 2, 3]
        //   cap = 8, len = 5, head = 4
        //   rc = 8 - 4 = 4, lc = 5 - 4 = 1
        let right_count = self.cap - self.head;
        let left_count = usize::saturating_sub(self.len, right_count);
        (right_count, left_count)
    }

    /// Return true is initialized items in `self.buf` are wrapped around the buffer.
    ///
    /// This means that buffer looks something like [initialized] [empty] [initialized]
    ///                                                                   ^- head
    /// Always `false` if `self.len <= 1` or can only be true if `self.len > 1`.
    fn is_wrapped(&self) -> bool {
        // * If cap == 0 then also head == len == 0 and 0 > 0 is false.
        // * If len == 0, then we return head > cap however our invariants state that
        //   head < cap, thus condition below is always false
        // * Similarly if len == 1, then head + 1 > cap is also always false,
        //   at best head + len == cap, but is never larger

        // An example:
        // [uninit, uninit, uninit, uninit, 0, 1, 2, 3]
        // cap = 8, len = 4, head = 4 => 4 + 4 > 8 == false, no wrapping
        // [5, uninit, uninit, uninit, 0, 1, 2, 3]
        // cap = 8, len = 5, head = 4 => 4 + 5 > 8 == true, wrapped
        self.head + self.len > self.cap
    }

    /// Returns a pointer to the head of vec in `self.buf`.
    ///
    /// The returned pointer is non-null and properly aligned.
    /// The pointed item is uninitialized if `self.len == 0`,
    /// but is guaranteed to be initialized otherwise.
    ///
    /// # SAFETY
    ///
    /// * `self.cap > 0` that is the buffer must have been allocated before calling this method
    unsafe fn head_ptr(&self) -> *mut T {
        // SAFETY:
        //  * self.head must be in bounds of self.buf after it's been allocated (see INVARIANTS)
        unsafe { self.get_raw_unchecked(self.head) }
    }

    /// Returns a pointer to item at `index` in `self.buf`.
    ///
    /// The returned pointer is non-null and properly aligned but the pointed
    /// item may be uninitialized.
    ///
    /// # SAFETY
    ///
    /// * `index` must be in bounds of buffer (`index < self.cap`)
    ///   Consequently this also implies that `self.buf` must have been allocated
    ///   and `self.cap > 0`.
    unsafe fn get_raw_unchecked(&self, index: usize) -> *mut T {
        // SAFETY:
        //  * `self.buf` is guaranteed to be initialized by caller and thus is a valid pointer
        //  * `self.buf` is valid pointer for `self.cap > index`
        //    `T`s so the resulting pointer is in bounds
        //  * computed offset `index * mem::size_of::<T>() < isize::MAX`
        //    because our allocation size `self.cap * mem::size_of::<T>()`
        //    is checked to be `< isize::MAX` in allocation code (see `self.grow_to`)
        unsafe { self.buf.as_ptr().add(index) }
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

            // SAFETY: `self.cap > 0` is checked above
            let right_start = unsafe { self.head_ptr().cast_const() };
            // SAFETY:
            //  * right_count is the number of initialized items from the head_ptr/right_start
            //  * left_count is the number of initialized items from the start of self.buf
            //  * head_ptr() returns properly aligned pointer and self.buf is properly aligned
            //  * all previously given out mutable references are bound to a mutable borrow of self,
            //    none of those can be alive
            //  * total size of creates slice cannot be larger than `isize::MAX` because
            //    our total allocation is smaller than that and these are subslices into it
            let right = unsafe { slice::from_raw_parts(right_start, right_count) };
            let left = unsafe { slice::from_raw_parts(self.buf.as_ptr(), left_count) };
            (right, left)
        } else {
            // SAFETY:
            //  * as self is not wrapped, there are self.len consecutive initialized ìtems
            //    starting at index self.head
            //  * points 3-5 apply from the same operation from if branch
            let right = unsafe { slice::from_raw_parts(self.head_ptr().cast_const(), self.len) };
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
        // SAFETY: `new_cap * mem::size_of<T>() > 0` because `new_cap > 0`
        //  and we don't support ZST
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

                // SAFETY:
                //  * right_count is the number of initialized items from the head_ptr/right_start
                //  * left_count is the number of initialized items from the start of self.buf
                //  * self.buf and buf are different allocations and don't overlap
                //  * new buf has capacity for more items than current buffer
                //  * self.buf is guaranteed to be aligned by our invariants,
                //    self.head_ptr() return aligned pointer,
                //    alloc returns aligned pointer
                //    and ptr::add preserves alignedness.
                unsafe { ptr::copy_nonoverlapping(self.head_ptr(), buf, right_count) };
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

                // SAFETY:
                //  * as self is not wrapped, there are self.len consecutive initialized ìtems
                //    starting at index self.head
                //  * points 3-5 apply from the same operation from previous branch
                unsafe { ptr::copy_nonoverlapping(self.head_ptr(), buf, self.len) }
            }

            // We haven't yet updated self.buf and self.cap
            let old_layout = self.current_layout();
            // SAFETY: buf is non-null in this branch
            let old_buf = mem::replace(&mut self.buf, unsafe {
                NonNull::new_unchecked(buf.cast::<T>())
            });
            let old_cap = mem::replace(&mut self.cap, new_cap);
            self.head = 0;

            if old_cap != 0 {
                // SAFETY:
                //  * we allocate only with Global allocator (we don't support custom allocators)
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
        let index = self.get_real_index(self.len);
        // SAFETY:
        //  * self.len > 0, thus get_real_index returns a valid index into self.buf
        //  * by taking &mut self, no-one else can have any references into self.buf
        //    thus whole buf is valid for us to write into
        unsafe { self.write_at(index, val) };
        // SAFETY:
        //  * index self.len points to the first uninitialized item, thus a write
        //    at that index keeps the initialized items contiguous
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
        // SAFETY:
        //  * since self.cap > 0, and self.head < self.cap, then index is in bound for self.buf
        //  * by taking &mut self, no-one else can have any references into self.buf
        //    thus whole buf is valid for us to write into
        unsafe { self.write_at(index, val) };
        // SAFETY:
        //  * index is always next to the current head, thus a write
        //    at that index keeps the initialized items contiguous
        self.len += 1;
        self.head = index;
    }

    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        // SAFETY:
        //  * since self.len > 0, the item at index self.head is initialized
        //  * self.head is shifted by 1 to the next element,
        //    so this item is never read again
        let val = unsafe { self.read_at(self.head) };
        // if new len == 0, self.head can be any index into our buffer
        self.head = if self.head == self.cap - 1 {
            // head was last element in out buffer, wrap around the buffer
            // [2, 3, uninit, 1], 1 is front, popped it, new head it at index 0
            0
        } else {
            self.head + 1
        };
        self.len -= 1;

        Some(val)
    }

    /// The actual index of an index'th element. Assumes that self is not empty
    /// and that the index is in bounds.
    ///
    /// If self is empty, the returned value has no real meaning and is the index
    /// is out of bounds it will wrap around the buffer potentially resulting in
    /// a index to random element or even to uninitialized element.
    #[inline]
    fn get_real_index(&self, index: usize) -> usize {
        debug_assert!(index < self.cap);
        (self.head + index) % self.cap
    }

    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let index = self.get_real_index(self.len - 1);
        // SAFETY:
        //  * since `self.len > 0`, the item at index `self.len - 1` is initialized
        //  * `self.len` is decremented by 1 so this item cannot be reached from
        //    `self.head` again
        let val = unsafe { self.read_at(index) };
        self.len -= 1;
        Some(val)
    }

    pub fn get(&mut self, index: usize) -> Option<&T> {
        if !self.is_in_bounds(index) {
            return None;
        }

        let index = self.get_real_index(index);
        // SAFETY: index is in bounds (checked above)
        let ptr = unsafe { self.get_raw_unchecked(index) };
        // SAFETY:
        //  * lifetime of returned reference is bound to the borrow of `self`, is must remain alive for '0
        //  * `ptr` is non-null as self.buf is non-null
        //  * `ptr` is properly aligned because self.buf is and ptr::add keeps it aligned
        //  * `ptr` points to a initialized T since `index < self.len` and first
        //    `self.len` items in `self.buf` are initialized (see INVARIANTS in struct definition)
        unsafe { Some(&*ptr) }
    }

    /// Overwrite location at `index` in `self.buf` with `val` without reading or dropping the old value.
    ///
    /// # SAFETY
    ///
    /// * `index < self.cap`
    /// * item at `index` must be valid to be written to
    /// * item at `index` should be uninitialized or an old sentinel value,
    ///   otherwise it would be leaked
    unsafe fn write_at(&mut self, index: usize, val: T) {
        // SAFETY: index is in bounds
        let ptr = unsafe { self.get_raw_unchecked(index) };
        // SAFETY:
        //  * get_raw_unchecked return non-null and properly aligned pointers into self.buf
        //  * any references given out before are invalidated by taking
        //    `&mut self` (all returned references are bound to a borrow of `self`)
        unsafe { ptr.write(val) };
    }

    /// Read the item at `index`.
    ///
    /// # SAFETY
    ///
    /// * item at `index` must be valid to be read
    /// * item at `index` must never be read from again
    unsafe fn read_at(&mut self, index: usize) -> T {
        // SAFETY: index is in bounds
        let ptr = unsafe { self.get_raw_unchecked(index) };
        // SAFETY:
        //  * this item will never be read again, only written over
        //  * `ptr` is valid to be read from
        //    - get_raw_unchecked return non-null and properly aligned pointers
        //    - any references given out before are invalidated by taking
        //      `&mut self` (all returned references are bound to a borrow of `self`)
        //  * `ptr` points to a properly initialized `T` since first `self.len`
        //    items in `self.buf` are initialized (see INVARIANTS in struct definition)
        unsafe { ptr.read() }
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
    use core::panic::AssertUnwindSafe;
    use core::sync::atomic::AtomicUsize;
    use std::panic::catch_unwind;

    use super::*;

    fn covariant<'a, T>(a: VecDeque2<&'static T>) -> VecDeque2<&'a T> {
        a
    }

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

    #[test]
    fn panic_in_drop() {
        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
        struct D(bool, String);

        impl Drop for D {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
                if self.0 {
                    panic!("panic from drop")
                }
            }
        }

        let mut v = VecDeque2::new();
        v.push_back(D(false, String::from("a")));
        v.push_back(D(true, String::from("b")));
        v.push_back(D(false, String::from("c")));

        catch_unwind(AssertUnwindSafe(|| drop(v))).ok();
        assert_eq!(DROP_COUNT.load(core::sync::atomic::Ordering::SeqCst), 3)
    }

    #[test]
    #[ignore = "should abort, needs to be manually checked"]
    fn panic_in_drop_abort() {
        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
        struct D(bool, String);

        impl Drop for D {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
                if self.0 {
                    panic!("panic from drop")
                }
            }
        }

        let mut v = VecDeque2::new();
        v.push_back(D(false, String::from("a")));
        v.push_back(D(true, String::from("b")));
        v.push_back(D(false, String::from("c")));
        v.push_back(D(true, String::from("d")));

        catch_unwind(AssertUnwindSafe(|| drop(v))).ok();
        assert_eq!(DROP_COUNT.load(core::sync::atomic::Ordering::SeqCst), 3)
    }
}
