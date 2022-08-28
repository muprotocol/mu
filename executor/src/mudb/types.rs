//! Internal module for types and traits

use sled::IVec;

pub trait ToFromIVec {
    type Type;
    fn from_ivec(ivec: &IVec) -> Self::Type;
    fn to_ivec(&self) -> IVec;
}

impl ToFromIVec for u64 {
    type Type = u64;

    fn from_ivec(ivec: &IVec) -> Self::Type {
        Self::from_le_bytes(ivec.as_ref().try_into().unwrap())
    }

    fn to_ivec(&self) -> IVec {
        self.to_le_bytes().as_ref().into()
    }
}

impl ToFromIVec for String {
    type Type = String;

    fn from_ivec(ivec: &IVec) -> Self::Type {
        Self::from_utf8(ivec.to_vec()).unwrap()
    }

    fn to_ivec(&self) -> IVec {
        self.as_str().into()
    }
}
