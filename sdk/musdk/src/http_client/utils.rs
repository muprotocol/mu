use std::borrow::Cow;

pub fn basic_auth<'a, U, P>(username: U, password: Option<P>) -> Cow<'a, str>
where
    U: std::fmt::Display,
    P: std::fmt::Display,
{
    use base64::prelude::BASE64_STANDARD;
    use base64::write::EncoderWriter;
    use std::io::Write;

    let mut buf = b"Basic ".to_vec();
    {
        let mut encoder = EncoderWriter::new(&mut buf, &BASE64_STANDARD);
        let _ = write!(encoder, "{}:", username);
        if let Some(password) = password {
            let _ = write!(encoder, "{}", password);
        }
    }

    String::from_bytes(&buf)
        .expect("base64 is always valid String")
        .into()
}
