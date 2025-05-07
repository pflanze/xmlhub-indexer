//! An abstraction of document sections:
//!
//! * that can be formatted for an HTML file or for a Markdown file
//!   with embedded HTML;
//! * that a table of contents can be built from (showing and linking the
//!   (possibly nested) subsections).

use ahtml::{att, AId, ASlice, HtmlAllocator, Node, Print, SerHtmlFrag};
use anyhow::Result;
use kstring::KString;
use rayon::{
    iter::ParallelIterator,
    prelude::{IndexedParallelIterator, IntoParallelRefIterator},
};

use crate::{string_tree::StringTree, xmlhub_indexer_defaults::HTML_ALLOCATOR_POOL};

#[derive(Clone, Copy, PartialEq)]
pub enum Highlight {
    None,
    Red,
    Orange,
}

impl Highlight {
    pub fn color_string(self) -> Option<&'static str> {
        match self {
            Highlight::None => None,
            Highlight::Red => Some("red"),
            Highlight::Orange => Some("orange"),
        }
    }

    /// Give attribute key-value pair for html elements
    pub fn color_att(self) -> Option<(KString, KString)> {
        self.color_string()
            .and_then(|color| att("style", format!("color: {color};")))
    }
}

/// A section consists of a (optional) section title, an optional
/// intro (that could be the only content), and a list of subsections
/// which could be empty. The toplevel section will not have a title,
/// but just a list of subsections.
pub struct Section {
    /// Whether to show the section title (both in the ToC and in the
    /// document body) in a highlighted way (different colour)
    pub highlight: Highlight,
    pub title: Option<String>,
    pub intro: Option<SerHtmlFrag>,
    pub subsections: Vec<Section>,
}

/// A list of section numbers (like "1.3.2") to identify a particular
/// subsection, used for naming them and linking from the table of
/// contents.
pub struct NumberPath {
    numbers: Vec<usize>,
}

impl NumberPath {
    pub fn empty() -> Self {
        Self {
            numbers: Vec::new(),
        }
    }
    /// Make a new path by adding an id to the end of the current one.
    fn add(&self, number: usize) -> Self {
        let mut numbers = self.numbers.clone();
        numbers.push(number);
        Self { numbers }
    }
    /// Gives e.g. `3` for a 3-level deep path
    fn level(&self) -> usize {
        self.numbers.len()
    }
    /// Gives e.g. `"1.3.2"`
    fn to_string(&self) -> String {
        self.numbers
            .iter()
            .map(|number| format!("{number}"))
            .collect::<Vec<String>>()
            .join(".")
    }
}

impl Section {
    /// Build a table of contents
    pub fn to_toc_html(&self, number_path: NumberPath, html: &HtmlAllocator) -> Result<AId<Node>> {
        let title_node = if let Some(title) = &self.title {
            let number_path_string = number_path.to_string();
            let section_id = format!("section-{number_path_string}");
            html.a(
                [
                    att("class", "toc_entry"),
                    self.highlight.color_att(),
                    att("href", format!("#{section_id}")),
                ],
                html.text(format!("{number_path_string} {title}"))?,
            )?
        } else {
            html.empty_node()?
        };
        let mut sub_nodes = html.new_vec();
        for (i, section) in self.subsections.iter().enumerate() {
            let id = i + 1;
            let sub_path = number_path.add(id);
            sub_nodes.push(section.to_toc_html(sub_path, html)?)?;
        }
        html.dl([], [html.dt([], title_node)?, html.dd([], sub_nodes)?])
    }

    /// Format the section for the inclusion in an HTML file
    pub fn to_html(&self, number_path: NumberPath, html: &HtmlAllocator) -> Result<ASlice<Node>> {
        let mut vec = html.new_vec();
        if let Some(title) = &self.title {
            // Choose the html method for the current nesting level by
            // indexing into the list of them, referring to the
            // methods via function reference syntax
            // (e.g. `html.h2(..)` has to be called right away,
            // `html.h2` alone is not valid, but `HtmlAllocator::h2`
            // gets a reference to the h2 method without calling it,
            // but it is now actually a reference to a normal
            // function, hence further down, `element` has to be
            // called with `html` as a normal function argument
            // instead).
            let elements = [
                HtmlAllocator::h1,
                HtmlAllocator::h2,
                HtmlAllocator::h3,
                HtmlAllocator::h4,
                HtmlAllocator::h5,
                HtmlAllocator::h6,
            ];
            let mut level = number_path.level();
            let max_level = elements.len() - 1;
            if level > max_level {
                level = max_level;
            }
            let element = elements[level];

            let number_path_string = number_path.to_string();
            let section_id = format!("section-{number_path_string}");
            vec.push(html.a([att("name", &section_id)], [])?)?;
            vec.push(element(
                html,
                [att("id", section_id), self.highlight.color_att()],
                // Prefix the path to the title; don't try to use CSS
                // as it won't make it through Markdown.
                html.text(format!("{number_path_string} {title}"))?,
            )?)?;
        }

        if let Some(fragment) = &self.intro {
            vec.push(html.preserialized(fragment.clone())?)?;
        }

        for (i, section) in self.subsections.iter().enumerate() {
            let id = i + 1;
            let sub_path = number_path.add(id);
            vec.push(html.div([], section.to_html(sub_path, html)?)?)?;
        }

        Ok(vec.as_slice())
    }

    /// Format the section for the inclusion in a markdown file
    pub fn to_markdown(&self, number_path: NumberPath) -> Result<StringTree> {
        let mut title_and_intro = String::new();
        if let Some(title) = &self.title {
            let number_path_string = number_path.to_string();
            let section_id = format!("section-{number_path_string}");
            let mut num_hashes = number_path.level() + 1;
            if num_hashes > 6 {
                num_hashes = 6
            }
            for _ in 0..num_hashes {
                title_and_intro.push('#');
            }
            title_and_intro.push(' ');
            // Add an anchor for in-page links; use both available
            // approaches, the older "name" and the newer "id"
            // approach, hoping that at least one gets through
            // GitLab's formatting.
            let html = HTML_ALLOCATOR_POOL.get();
            title_and_intro.push_str(
                &html
                    .a([att("name", &section_id), att("id", &section_id)], [])?
                    .to_html_fragment_string(&html)?,
            );
            title_and_intro.push_str(&number_path_string);
            title_and_intro.push(' ');
            // Should we use HTML to try to make this red if
            // `self.in_red`? But GitLab drops it anyway, and there's
            // risk of messing up the title display.
            title_and_intro.push_str(title);
            title_and_intro.push_str("\n\n");
        }

        if let Some(fragment) = &self.intro {
            title_and_intro.push_str(fragment.as_str());
            title_and_intro.push_str("\n\n");
        }

        let sub_trees = self
            .subsections
            .par_iter()
            .enumerate()
            .map(|(i, section)| {
                let id = i + 1;
                let sub_path = number_path.add(id);
                section.to_markdown(sub_path)
            })
            .collect::<Result<_>>()?;

        Ok(StringTree::Branching(vec![
            StringTree::Leaf(title_and_intro),
            StringTree::Branching(sub_trees),
        ]))
    }
}
