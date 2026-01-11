/*
https://www.doi.org/doi-handbook/HTML/doi-name-syntax2.html

The DOI syntax follows the Handle System's rules and is as follows:

<prefix>/<suffix>

where:

•prefix: refers to the DOI namespace (a namespace is allocated to a given service provider)
The prefix can contain only numeric values and the "." character which is used to delimit a hierarchical level in the namespace allocation: a one-delimiter prefix (for example, "10.1000") derives from a zero-delimiter prefix ("10").
In the Handle System, the prefix 10 is allocated to the DOI Foundation.

•suffix: is a unique local name in the namespace
Any Unicode 2.0 character can be used in the suffix (there is no practical limitation on the length of a DOI name). This unique string may be an existing identifier, or any unique string chosen by the Registration Agency or the referent owner (registrant).
# no *practical* limitation are you kidding  me ?
#omygd

DOI name examples are: 10.1000/xyz-123; 10.1109/5.771073; etc.

NOTE DOI names may also be expressed as URLs: see Direct Redirection to a Web Resource with the DOI Proxy.

For more information about the DOI name syntax, see DOI Namespace.

 */

// Are multiple delimiters allowed or not ?

use std::{
    fmt::{Debug, Display, Write},
    str::FromStr,
};

use auri::url_encoding::url_encode;

#[derive(Debug, PartialEq, Eq)]
pub struct Doi<S> {
    prefix: Vec<S>,
    suffix: S,
    len: u16,
}

impl<S> Doi<S> {
    pub fn len(&self) -> usize {
        self.len.into()
    }
}

// /// Only for valid doi_string
// pub fn unchecked_doi_uri_from_str(doi_string: &str) -> String {
//     // let ppath = PPath::new(false, false, vec![KString::from_ref(doi_string)]);
//     // let urlpath = AUriLocal::new(ppath, None);
//     // String::from(urlpath)
//     // ^ Ugh, auri does need lots of work! Contorted API and then it's wrong.
//     format!("https://doi.org/{}", url_encode(doi_string))
// }
// Wrong, since it encodes the '/' in the middle, too.

impl<S: AsRef<str>> Doi<S> {
    pub fn url(&self) -> String {
        // unchecked_doi_uri_from_str(&self.to_string())
        let mut out = String::from("https://doi.org/");
        out_string_join(&mut out, &self.prefix, '.');
        out.push('/');
        out.push_str(&url_encode(self.suffix.as_ref()));
        out
    }
}

fn out_string_join<S: AsRef<str>>(out: &mut String, ss: &[S], gap: char) {
    let mut first = true;
    for s in ss {
        if first {
            first = false;
        } else {
            out.push(gap);
        }
        out.push_str(s.as_ref());
    }
}

fn display_join<S: Display>(
    f: &mut std::fmt::Formatter<'_>,
    ss: &[S],
    gap: char,
) -> std::fmt::Result {
    let mut first = true;
    for s in ss {
        if first {
            first = false;
        } else {
            f.write_char(gap)?;
        }
        f.write_fmt(format_args!("{}", s))?;
    }
    Ok(())
}

impl<S: Display> Display for Doi<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_join(f, &self.prefix, '.')?;
        f.write_fmt(format_args!("/{}", self.suffix))
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum DoiParseError {
    #[error("missing slash")]
    MissingSlash,
    #[error("suffix missing or too short")]
    SuffixTooShort,
    #[error("invalid character {1:?} in prefix at position {1}")]
    InvalidPrefixChar(char, u16),
    #[error("DOI too long")]
    TooLong,
    #[error("DOI suffix has whitespace at position {0}")]
    WhitespaceInSuffix(u16),
}

// In UTF-8 bytes. Randomly chosen.
const MAX_LEN: u16 = 400;
// In UTF-8 bytes. I am just assuming.
const MIN_SUFFIX_LEN: usize = 1;

impl<'s, S> Doi<S>
where
    S: From<&'s str>,
    S: AsRef<str>,
{
    pub fn parse_str(s: &'s str) -> Result<(Self, &'s str), DoiParseError> {
        if s.len() > MAX_LEN.into() {
            return Err(DoiParseError::TooLong);
        }
        if let Some(bytepos) = s.find('/') {
            let prefix_str = &s[..bytepos];
            let prefix: Vec<S> = prefix_str.split('.').map(From::from).collect();
            let mut charpos = 0;
            {
                for p in &prefix {
                    for c in p.as_ref().chars() {
                        if !c.is_ascii_digit() {
                            return Err(DoiParseError::InvalidPrefixChar(c, charpos));
                        }
                        charpos += 1;
                    }
                    charpos += 1; // for the '.' or '/'
                }
            }
            let rest = &s[bytepos + 1..];
            let suffix_endpos = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let suffix = &rest[..suffix_endpos];
            if suffix.len() < MIN_SUFFIX_LEN {
                return Err(DoiParseError::SuffixTooShort);
            }
            let len: u16 = (prefix_str.len() + 1 + suffix.len())
                .try_into()
                .expect("len checked originally");
            Ok((
                Self {
                    prefix,
                    suffix: suffix.into(),
                    len,
                },
                &rest[suffix_endpos..],
            ))
        } else {
            Err(DoiParseError::MissingSlash)
        }
    }
}

impl FromStr for Doi<String> {
    type Err = DoiParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (slf, rest) = Self::parse_str(s)?;
        if rest.is_empty() {
            Ok(slf)
        } else {
            let before_rest = &s[0..s.len() - rest.len()];
            Err(DoiParseError::WhitespaceInSuffix(
                before_rest
                    .chars()
                    .count()
                    .try_into()
                    .expect("fits because controlled len before"),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_valid() {
        for s in [
            "10.1000/xyz-123",
            "10.1109/5.771073",
            // Random guesses for what should be valid, according to
            // the above claims?:
            "10.1000//xyz-123",
            "10.1109/1",
            "0.1109/1",
        ] {
            match Doi::from_str(s) {
                Ok(doi) => {
                    assert_eq!(format!("{doi}"), s);
                    assert_eq!(doi.len(), s.len());
                }
                Err(e) => panic!("erroneously not parsed: {s:?}, {e:#}"),
            }
        }
    }

    #[test]
    fn t_valid_parse() {
        for (s, printed, rest) in [
            ("10.1000/xyz-123", "10.1000/xyz-123", ""),
            ("10.1109/5.771073 there", "10.1109/5.771073", " there"),
        ] {
            match Doi::<&str>::parse_str(s) {
                Ok((doi, rest2)) => {
                    assert_eq!(format!("{doi}"), printed, "for s {s:?}");
                    assert_eq!(doi.len(), printed.len(), "for s {s:?}");
                    assert_eq!(rest2, rest, "for s {s:?}");
                }
                Err(e) => panic!("erroneously not parsed: {s:?}, {e:#}"),
            }
        }
    }

    #[test]
    fn t_invalid() {
        for (s, e) in [
            ("10.1000|xyz-123", DoiParseError::MissingSlash),
            ("10.1000 /xyz-123", DoiParseError::InvalidPrefixChar(' ', 7)),
            ("10_1000/xyz-123", DoiParseError::InvalidPrefixChar('_', 2)),
            ("10.1109/", DoiParseError::SuffixTooShort),
            // I expect the wording to be wrong and whitespace is not
            // acceptable:
            ("10.1000/xyz-123 ", DoiParseError::WhitespaceInSuffix(15)),
            // Heh, parse_str doesn't know we know the len in advance,
            // thus we get SuffixTooShort, not WhitespaceInSuffix(8)
            // here:
            ("10.1000/ xyz-123", DoiParseError::SuffixTooShort),
            ("10.1000/x yz-123", DoiParseError::WhitespaceInSuffix(9)),
            ("10.1000/xyz-123ä ", DoiParseError::WhitespaceInSuffix(16)),
        ] {
            let res = Doi::from_str(s);
            assert!(res.is_err(), "accidentally parsed: {s:?}");
            let got_e = res.err().unwrap();
            assert_eq!(got_e, e, "for string {s:?}");
        }
    }
}
