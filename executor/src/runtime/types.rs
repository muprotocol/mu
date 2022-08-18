use serde::{Deserialize, Serialize};
use std::any::type_name;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

#[derive(Deserialize, Serialize)]
pub struct ID<T> {
    inner: [u8; 32],
    #[serde(skip)]
    phantom_type: PhantomData<T>,
}

impl<T> ID<T> {
    pub fn new(id: [u8; 32]) -> Self {
        Self {
            inner: id,
            phantom_type: PhantomData,
        }
    }

    pub fn inner_to_string(&self) -> String {
        unimplemented!()
    }
}

impl<T> Hash for ID<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state)
    }
}

impl<T> PartialEq for ID<T> {
    fn eq(&self, other: &Self) -> bool {
        self.phantom_type == other.phantom_type && self.inner == other.inner
    }
}

impl<T> Eq for ID<T> {}

impl<T> Clone for ID<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner,
            phantom_type: PhantomData,
        }
    }
}

impl<T> Copy for ID<T> {}

impl<T> Debug for ID<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[")?;
        f.write_str(type_name::<T>())?;
        f.write_str("] ")?;
        self.inner.fmt(f)
    }
}
