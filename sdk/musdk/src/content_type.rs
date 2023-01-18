pub type Mime<'a> = &'a str;
pub type Charset<'a> = &'a str;

pub fn parse(header: &str) -> (Option<Mime>, Option<Charset>) {
    let mut parts = header.split(';').map(|s| s.trim());

    let mime = match parts.next() {
        None => None,
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
    };

    let charset = parts
        .filter_map(|p| {
            let s: Vec<&str> = p.split('=').collect();
            if s.len() == 2 {
                Some((s[0], s[1]))
            } else {
                None
            }
        })
        .find_map(|(k, v)| {
            if k.to_lowercase() == "charset" {
                Some(v)
            } else {
                None
            }
        });

    (mime, charset)
}

#[cfg(test)]
mod tests {
    use crate::content_type::parse;

    #[test]
    fn test_parsing() {
        assert_eq!(
            parse("text/plain; charset=utf-8"),
            (Some("text/plain"), Some("utf-8"))
        );

        assert_eq!(
            parse("text/plain;charset=utf-8"),
            (Some("text/plain"), Some("utf-8"))
        );

        assert_eq!(
            parse("TEXT/PLAIN;CHARSET=UTF-8"),
            (Some("TEXT/PLAIN"), Some("UTF-8"))
        );

        assert_eq!(parse("text/plain"), (Some("text/plain"), None));
        assert_eq!(parse("text/plain;"), (Some("text/plain"), None));
        assert_eq!(parse(""), (None, None));
        assert_eq!(parse(";charset=utf-8"), (None, Some("utf-8")));
    }
}
