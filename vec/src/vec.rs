extern crate alloc as crate_alloc;

use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::{fmt, mem, ptr, slice};

use crate_alloc::alloc;

struct Vec2<T> {
    /// INVARIANTS:
    ///  * `len <= cap`
    ///  * first `len` elements in `buf` are initialized
    ///  * `buf` is valid pointer to contiguous memory to store `cap` `T`s
    ///    (`buf` can only be `NonNull::dangling` if `cap == len == 0`)
    //
    buf: NonNull<T>,
    len: usize,
    cap: usize,
    marker: PhantomData<T>,
}

fn covariant<'a, T>(a: Vec2<&'static T>) -> Vec2<&'a T> {
    a
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
        if self.cap == 0 {
            return;
        }

        /// Drop guard in case T::drop panics.
        ///
        /// In the case on unwinding we try to drop the remaining items.
        /// If that succeeds we deallocate our buffer and the caller could catch the unwinding,
        /// if not we abort due to double panic.
        struct Guard<'a, U>(&'a mut Vec2<U>);

        impl<'a, U> Drop for Guard<'a, U> {
            fn drop(&mut self) {
                while self.0.pop().is_some() {}

                assert_eq!(self.0.len, 0);

                let layout = self.0.current_layout();
                self.0.cap = 0;
                let buf = mem::replace(&mut self.0.buf, NonNull::dangling())
                    .as_ptr()
                    .cast::<u8>();

                unsafe { alloc::dealloc(buf, layout) };
            }
        }

        let g = Guard(self);
        while g.0.pop().is_some() {}
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
            // SAFETY: self.buf is never touched before actually initializing it
            buf: NonNull::dangling(),
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
        // SAFETY:
        //  * if `len == cap == 0` then `self.buf == NonNull::dangling`,
        //    this is valid pointer for zero-len slice (see docs of `slice::from_raw_parts`)
        //  * otherwise `self.buf` is a valid pointer to `self.len` `T`s
        //    gotten from `alloc::alloc` with `Layout::array<T>(cap)` which is non-null and properly aligned.
        //    First `self.len` `T`s in that memory are properly initialized.
        unsafe { slice::from_raw_parts(self.buf.as_ptr().cast_const(), self.len) }
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

        let (buf, layout) = if self.cap == 0 {
            let layout = Layout::array::<T>(new_cap).unwrap();
            debug_assert_ne!(layout.size(), 0);
            // SAFETY: `new_cap * mem::size_of<T>() > 0` because `new_cap > 0`
            //  (new_cap > cap == 0 by combining two if statements) and we
            //  don't support ZST
            let buf = unsafe { alloc::alloc(layout) };
            (buf, layout)
        } else {
            let new_layout = Layout::array::<T>(new_cap).unwrap();
            // SAFETY:
            //  * we allocate only with Global allocator (we don't support custom allocators)
            //  * `self.current_layout()` returns the layout of current `self.buf`
            //  * `new_size = new_layout.size() > 0` because (`new_cap > cap != 0`) and we don't support ZST
            //  * `new_size = new_layout.size() < isize::MAX` because `Layout::array` would panic if this is not the case.
            let buf = unsafe {
                alloc::realloc(
                    self.buf.as_ptr().cast::<u8>(),
                    self.current_layout(),
                    new_layout.size(),
                )
            };
            (buf, new_layout)
        };

        if buf.is_null() {
            alloc::handle_alloc_error(layout)
        } else {
            // SAFETY:
            //  * we just checked that buf is not null.
            self.buf = unsafe { NonNull::new_unchecked(buf.cast::<T>()) };
            self.cap = new_cap;
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

    pub fn push(&mut self, val: T) {
        if self.len == self.cap {
            self.grow()
        }

        assert!(self.len < self.cap);
        // SAFETY:
        //  * `self.buf` is valid pointer for `self.cap > self.len` `T`s so the resulting pointer is in bounds
        //  * computed offset `self.len * mem::size_of::<T>() < isize::MAX`
        //    because our allocation size `self.cap * mem::size_of::<T>()`
        //    is checked to be `< isize::MAX` in allocation code (see `self.grow_to`)
        let ptr = unsafe { self.buf.as_ptr().add(self.len) };
        // SAFETY:
        //  * `ptr` is valid for writes because it's derived from `NonNull<T>` to the
        //    whole buffer and we haven't given out a reference to that location before
        //  * `ptr` is properly aligned because `self.buf` is properly aligned and `ptr::add` keeps the pointer aligned
        unsafe { ptr.write(val) };
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        // Want to read at last index, so decrement before reading
        self.len -= 1;
        // SAFETY:
        //  * `self.buf` is valid pointer for `self.cap > self.len` `T`s so the resulting pointer is in bounds
        //  * computed offset `self.len * mem::size_of::<T>() < isize::MAX`
        //    because our allocation size `self.cap * mem::size_of::<T>()`
        //    is checked to be `< isize::MAX` in allocation code (see `self.grow_to`)
        let ptr = unsafe { self.buf.as_ptr().add(self.len) };
        // SAFETY:
        //  * this item will never be read again, only written over
        //  * `ptr` is valid to be read from
        //    - it is properly aligned because self.buf is and ptr::add keeps it aligned
        //    - it is non-null because self.buf is non-null
        //    - any references given out before are invalidated by taking
        //      `&mut self` (all returned references are bound to a borrow of `self`)
        //  * `ptr` points to a properly initialized `T` since first `self.len`
        //    items in `self.buf` are initialized (see INVARIANTS in struct definition)
        let val = unsafe { ptr.read() };
        Some(val)
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if !self.is_in_bounds(index) {
            return None;
        }

        // SAFETY:
        //  * `self.buf` is valid pointer for `self.cap > self.len` `T`s so the resulting pointer is in bounds
        //  * computed offset `self.len * mem::size_of::<T>() < isize::MAX`
        //    because our allocation size `self.cap * mem::size_of::<T>()`
        //    is checked to be `< isize::MAX` in allocation code (see `self.grow_to`)
        let ptr = unsafe { self.buf.as_ptr().add(index) };
        // SAFETY:
        //  * lifetime of returned reference is bound to the borrow of `self`, is must remain alive for '0
        //  * `ptr` is non-null as self.buf is non-null
        //  * `ptr` is properly aligned because self.buf is and ptr::add keeps it aligned
        //  * `ptr` points to a initialized T since `index < self.len` and first
        //    `self.len` items in `self.buf` are initialized (see INVARIANTS in struct definition)
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

        // SAFETY:
        //  * `self.buf` is valid pointer for `self.cap > self.len > index` `T`s so the resulting pointer is in bounds
        //  * computed offset `index * mem::size_of::<T>() < isize::MAX`
        //    because our allocation size `self.cap * mem::size_of::<T>()`
        //    is checked to be `< isize::MAX` in allocation code (see `self.grow_to`)
        let ptr = unsafe { self.buf.as_ptr().add(index) };
        // SAFETY:
        //  * this item will never be read again, only written over
        //  * `ptr` is valid to be read from
        //    - it is properly aligned because self.buf is and ptr::add keeps it aligned
        //    - it is non-null because self.buf is non-null
        //    - any references given out before are invalidated by taking
        //      `&mut self` (all returned references are bound to a borrow of `self`)
        //  * `ptr` points to a properly initialized `T` since first `self.len`
        //    items in `self.buf` are initialized (see INVARIANTS in struct definition)
        let val = unsafe { ptr.read() };

        // shift tail down by 1 position
        // [head] [empty_slot] [tail]     [after]
        //        ^-index      ^-index+1  ^-self.len
        self.len -= 1;
        // Number of items in tail: if we removed the last item then index = orig_len - 1 = self.len.
        // In that case tail_count must equal 0, thus tail_count = self.len - index = orig_len - 1 - index
        let tail_count = self.len - index;
        if tail_count > 0 {
            unsafe {
                // SAFETY: since tail_count > 0 there must be at least one item
                //  after index, thus ptr.add(1) must also be in bounds for self.buf
                let tail_start = ptr.add(1);
                // SAFETY:
                //  * src and dst may overlap, use ptr::copy
                //  * tail_start points to a first valid item after the removed one
                //  * tail_start is valid to be read tail_count items because
                //    index + 1 + tail_count = orig_len which must be valid initialized `T`s
                //  * `ptr` is valid to write tail_count items because all these positions were previously initialized and thus were checked to be valid.
                //    And by taking `&mut self` we invalidate any previously returned references to the items which will be moved.
                //  * `ptr` is properly aligned because `self.buf` is properly aligned and `ptr::add` keeps the pointer aligned
                ptr::copy(tail_start, ptr, tail_count)
            }
        }

        // SAFETY:
        //  * we have shifted down the tail so at this point again self.len first
        //    items in self.buf are initialized and all our invariants hold

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

            // SAFETY:
            //  * `self.buf` is valid pointer for `self.cap > self.len > index` `T`s so the resulting pointer is in bounds
            //  * computed offset `index * mem::size_of::<T>() < isize::MAX`
            //    because our allocation size `self.cap * mem::size_of::<T>()`
            //    is checked to be `< isize::MAX` in allocation code (see `self.grow_to`)
            let tail_start = self.buf.as_ptr().add(index);
            let tail_count = self.len - index;
            // SAFETY:
            //  * we checked that there is capacity for 1 more item
            //  * tail_start is valid to be read tail_count items because
            //    index + tail_count = self.len which must be valid initialized `T`s
            ptr::copy(tail_start, tail_start.add(1), tail_count)
            // [head] [empty]  [tail] [after]
            //        ^-index         ^-self.len
        }

        unsafe {
            // write new value to buf[index]
            // SAFETY: see comment on `tail_start` above
            let ptr = self.buf.as_ptr().add(index);
            // SAFETY:
            //  * `ptr` is valid for writes because it's derived from `NonNull<T>` to the
            //    whole buffer and we haven't given out a reference to that location before
            //  * `ptr` is properly aligned because `self.buf` is properly aligned and `ptr::add` keeps the pointer aligned
            ptr.write(val);
        }

        self.len += 1;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::panic::AssertUnwindSafe;
    use core::sync::atomic::AtomicUsize;
    use std::panic::catch_unwind;

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
        println!("{:?}", v);
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

        let mut v = Vec2::new();
        v.push(D(false, String::from("a")));
        v.push(D(true, String::from("b")));
        v.push(D(false, String::from("c")));

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

        let mut v = Vec2::new();
        v.push(D(false, String::from("a")));
        v.push(D(true, String::from("b")));
        v.push(D(false, String::from("c")));
        v.push(D(true, String::from("d")));

        catch_unwind(AssertUnwindSafe(|| drop(v))).ok();
        assert_eq!(DROP_COUNT.load(core::sync::atomic::Ordering::SeqCst), 3)
    }
}
