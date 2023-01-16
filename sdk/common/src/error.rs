pub struct StatusError<T, const C: u16>(T)
where
    T: Into<Vec<u8>>;
