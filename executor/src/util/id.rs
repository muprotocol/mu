use std::ops::AddAssign;

use num::One;

pub trait IdExt: AddAssign + Copy + One {
    fn get_and_increment(&mut self) -> Self {
        let res = *self;
        *self += One::one();
        res
    }
}

impl<T: AddAssign + Copy + One> IdExt for T {}
