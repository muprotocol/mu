use std::ptr;

pub mod id;

pub trait ReplaceWithDefault: Default {
    /// This function is used to take ownership of a value inside a
    /// mutable reference that we know is going to be dropped, such
    /// as the value of a map key we're about to replace.
    /// This isn't a very elegant solution though, so if anybody
    /// knows a better way, please let me know.
    fn take_and_replace_default(&mut self) -> Self {
        let temp = Self::default();
        std::mem::replace(self, temp)
    }
}

impl<T: Default> ReplaceWithDefault for T {}

pub trait ReplaceWith: Sized {
    /// This function is used to take ownership of a value inside a
    /// mutable reference that we know is going to be dropped, such
    /// as the value of a map key we're about to replace.
    /// This isn't a very elegant solution though, so if anybody
    /// knows a better way, please let me know.
    /// This takes the replacement value as a parameter instead of
    /// relying of `std::default::Default`.
    fn take_and_replace_with(&mut self, default: Self) -> Self {
        std::mem::replace(self, default)
    }

    /// This function passes ownership of the value contained in the
    /// reference to a transformation function and replaces the value
    /// in the reference with the new value.
    fn replace_value(&mut self, f: impl FnOnce(Self) -> Self) {
        unsafe {
            // Safety: the old, duplicated value is overwritten immediately before
            // control leaves this function, so nothing is duplicated.
            ptr::write(self, f(ptr::read(self)));
        }
    }
}

impl<T: Sized> ReplaceWith for T {}
