use ahtml::{att, ASlice, HtmlAllocator, Node};
use anyhow::Result;

use crate::doi::Doi;

/// What kind of link auto-generation should be done, if any
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autolink {
    /// No auto-linking
    None,
    /// Recognize and link http and https URLs (e.g. the string `"See
    /// https://example.com."` is turned into the HTML code `See
    /// <a href="https://example.com">https://example.com</a>.`)
    Web,
    /// Recognize and link DOI identifiers to the corresponding entry
    /// on the https://doi.org/ website
    Doi,
}

impl Autolink {
    /// Text for display in web or command line
    pub fn to_text(self) -> &'static str {
        match self {
            Autolink::None => "no linking",
            Autolink::Web => "link web URLs",
            Autolink::Doi => "link DOI identifiers",
        }
    }

    /// Do not use `SoftPre`, only do the autolinking.
    pub fn format_html(self, text: &str, html: &HtmlAllocator) -> Result<ASlice<Node>> {
        match self {
            Autolink::None => html.text_slice(text),
            Autolink::Web => ahtml::util::autolink(html, text),
            Autolink::Doi => doi_autolink(text, html),
        }
    }
}

fn first_char_byte_count(s: &str) -> usize {
    match s.char_indices().nth(1) {
        Some((index, _)) => index,
        None => s.len(),
    }
}

pub fn doi_autolink<'t>(input: &'t str, html: &HtmlAllocator) -> Result<ASlice<Node>> {
    // Find instances like `10.1144/SP549-2023-174`
    let mut vec = html.new_vec();
    let mut text_start = 0;
    let mut i = 0;
    loop {
        if i == input.len() {
            break;
        }
        if let Ok((doi, _)) = Doi::<&'t str>::parse_str(&input[i..]) {
            if i > text_start {
                vec.push(html.text(&input[text_start..i])?)?;
                text_start = i;
            }
            i += doi.len();

            let doi_str = &input[text_start..i];
            vec.push(html.a([att("href", doi.url())], html.text(doi_str)?)?)?;
            text_start = i;
        } else {
            i += first_char_byte_count(&input[i..])
        }
    }
    if i > text_start {
        vec.push(html.text(&input[text_start..i])?)?;
    }

    Ok(vec.as_slice())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ahtml::Print;

    use super::*;

    #[test]
    fn t_() -> Result<()> {
        let mut html = HtmlAllocator::new(10000, Arc::new(String::from("foo")));
        let mut t = |s: &str| -> Result<String> {
            let l = doi_autolink(s, &mut html)?;
            Ok(html.div([], l)?.to_html_fragment_string(&mut html)?)
        };
        assert_eq!(t("Hi there")?, "<div>Hi there</div>");
        assert_eq!(
            t("10.1000/xyz-123")?,
            "<div><a href=\"https://doi.org/10.1000/xyz-123\">10.1000/xyz-123</a></div>"
        );
        assert_eq!(
            t("Hi 10.1000/xyz?123+%12&1 there")?,
            "<div>Hi <a href=\"https://doi.org/10.1000/xyz%3F123%2B%2512%261\">\
             10.1000/xyz?123+%12&amp;1</a> there</div>"
        );
        assert_eq!(
            t("Hi 10.1000/xyz-123 there 30/samba")?,
            "<div>Hi <a href=\"https://doi.org/10.1000/xyz-123\">\
             10.1000/xyz-123</a> there <a href=\"https://doi.org/30/samba\">\
             30/samba</a></div>"
        );

        Ok(())
    }
}
