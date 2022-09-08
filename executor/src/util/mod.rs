pub mod id;

pub trait TakeAndReplaceWithDefault: Default {
    /// This function is used to take ownership of a value inside a
    /// mutable reference that we know is going to be dropped, such
    /// as the value of a map key we're about to replace.
    /// This isn't a very elegant solution though, so if anybody
    /// knows a better way, please let me know.
    fn take_and_replace_with_default(&mut self) -> Self {
        let temp = Self::default();
        std::mem::replace(self, temp)
    }
}

impl<T: Default> TakeAndReplaceWithDefault for T {}
