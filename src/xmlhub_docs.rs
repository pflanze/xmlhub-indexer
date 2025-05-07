use std::{
    ffi::OsString,
    io::{stdout, Write},
    path::PathBuf,
};

use ahtml::{att, flat::Flat, AId, HtmlAllocator, Node, Print};
use ahtml_from_markdown::markdown::markdown_to_html;
use anyhow::Result;
use itertools::intersperse_with;

use crate::{
    browser::{spawn_browser, spawn_browser_on_path},
    installation::defaults::global_app_state_dir,
    path_util::AppendToPath,
    string_tree::StringTree,
    xmlhub_attributes::{specifications_to_html, METADATA_SPECIFICATION},
    xmlhub_global_opts::OpenOrPrintOpts,
    xmlhub_help::save_basic_standalone_html_page,
    xmlhub_indexer_defaults::{GENERATED_MESSAGE, HTML_ALLOCATOR_POOL},
};

pub fn flatten_as_paragraphs(vecs: Vec<Vec<StringTree>>) -> Vec<StringTree> {
    intersperse_with(vecs.into_iter().flatten(), || "\n\n".into()).collect()
}

/// The file name (without the .md or .html suffix) of the file with
/// information on how to contribute.
pub const CONTRIBUTE_FILENAME: &str = "CONTRIBUTE";

/// Build the contents for the ATTRIBUTES_FILE
pub fn make_attributes_md(link_contribute_file: bool) -> Result<StringTree<'static>> {
    let html = HTML_ALLOCATOR_POOL.get();

    let spec_html = specifications_to_html(&html)?.to_html_fragment_string(&html)?;

    let link_to_contribute_file = html
        .a(
            [att("href", format!("{}.md", CONTRIBUTE_FILENAME))],
            html.text(format!("{}", CONTRIBUTE_FILENAME))?,
        )?
        .to_html_fragment_string(&html)?;

    Ok(StringTree::Branching(flatten_as_paragraphs(vec![vec![
        format!(
            "<!-- NOTE: {}, do not edit manually! -->",
            *GENERATED_MESSAGE
        )
        .into(),
        format!("# Metainfo attributes").into(),
        format!(
            "This describes how each attribute from the XML file headers {} is interpreted. \
             `required` means that an actual non-empty value is required, just the \
             presence of the attribute is not enough.",
            if link_contribute_file {
                format!("(as described by {link_to_contribute_file})")
            } else {
                "".into()
            }
        )
        .into(),
        "(If you have a suggestion for another metadata field, tell your XML Hub maintainer!)"
            .into(),
        "Note: you can use the xmlhub command line tool, via `xmlhub prepare` or \
         `xmlhub add-to`, to get a template of these attributes into your file, \
         so you don't have to add these headers individually yourself!"
            .into(),
        spec_html.into(),
    ]])))
}

struct HelpPageInfo {
    which_page: WhichPage,
    file_name: &'static str,
    title: &'static str,
    body: AId<Node>,
}

macro_rules! def_enum_with_list{
    { $t:tt { $($case:tt,)* } } => {
        #[derive(Clone, Copy, PartialEq)]
        pub enum $t {
            $($case,)*
        }
        impl $t {
            fn list() -> &'static [$t] {
                use $t::*;
                &[$($case,)*]
            }
        }
    }
}

// Choice of a particular page from the set of help pages.
def_enum_with_list!(WhichPage {
    Start,
    Attributes,
    MacOS,
});

impl WhichPage {
    fn create_page(self, html: &HtmlAllocator) -> Result<HelpPageInfo> {
        match self {
            WhichPage::Start => {
                let body = markdown_to_html(include_str!("../docs/start.md"), &html)?.html();
                Ok(HelpPageInfo {
                    which_page: self,
                    file_name: "start.html",
                    title: "Start",
                    body,
                })
            }
            WhichPage::Attributes => {
                let body = markdown_to_html(&make_attributes_md(false)?.to_string(), &html)?.html();
                Ok(HelpPageInfo {
                    which_page: self,
                    file_name: "attributes.html",
                    title: "Attributes list",
                    body,
                })
            }
            WhichPage::MacOS => {
                let body = markdown_to_html(include_str!("../docs/macos.md"), &html)?.html();
                Ok(HelpPageInfo {
                    which_page: self,
                    file_name: "macos.html",
                    title: "macOS",
                    body,
                })
            }
        }
    }
}

// Create multiple/all help pages, so that they can link to each
// other! Returns the path to the page for which you passed the
// `WhichPage`.
fn create_help_pages(give_which_page: WhichPage, program_version: &str) -> Result<PathBuf> {
    let html = HTML_ALLOCATOR_POOL.get();

    let output_path_base = global_app_state_dir()?.docs_base(program_version)?;

    let page_infos: Vec<HelpPageInfo> = WhichPage::list()
        .iter()
        .map(|which| which.create_page(&html))
        .collect::<Result<_>>()?;

    let nav_for_page = |this_page: &HelpPageInfo| -> Result<AId<Node>> {
        let mut items = html.new_vec();
        let mut is_first = true;
        for pi in &page_infos {
            if is_first {
                is_first = false;
            } else {
                items.push(html.text(" | ")?)?;
            }
            let item_text = html.text(pi.title)?;
            let item = if pi.which_page == this_page.which_page {
                item_text
            } else {
                html.a([att("href", pi.file_name)], item_text)?
            };
            items.push(item)?;
        }
        html.div([att("class", "nav")], items)
    };

    let pages: Vec<(WhichPage, PathBuf)> = page_infos
        .iter()
        .map(|page_info| {
            let output_path = (&output_path_base).append(page_info.file_name);

            let nav = nav_for_page(page_info)?;
            let body = Flat::Two(nav, page_info.body);

            save_basic_standalone_html_page(&output_path, page_info.title, body, &html)?;

            Ok((page_info.which_page, output_path))
        })
        .collect::<Result<_>>()?;

    Ok(pages
        .into_iter()
        .find(|(k, _)| *k == give_which_page)
        .expect("all possible pages created above")
        .1)
}

pub fn open_help_page(which_page: WhichPage, program_version: &str) -> Result<()> {
    let output_path = create_help_pages(which_page, program_version)?;
    spawn_browser_on_path(&output_path)
}

pub fn docs_command(program_version: &str) -> Result<()> {
    open_help_page(WhichPage::Start, program_version)
}

pub fn help_contributing_command() -> Result<()> {
    // XX sigh, spawn_browser is badly prepared for external urls,
    // (1) should not need a directory, (2) should not require
    // arguments to be OsStr.
    spawn_browser(
        &PathBuf::from("/"),
        &[&OsString::from(
            "https://cevo-git.ethz.ch/cevo-resources/xmlhub/-/blob/master/CONTRIBUTE.md\
             ?ref_type=heads",
        )],
    )?;
    Ok(())
}

#[derive(clap::Parser, Debug)]
pub struct HelpAttributesOpts {
    #[clap(flatten)]
    open_or_print: OpenOrPrintOpts,
}

pub fn help_attributes_command(
    command_opts: HelpAttributesOpts,
    program_version: &str,
) -> Result<()> {
    let HelpAttributesOpts { open_or_print } = command_opts;

    if open_or_print.do_open() {
        open_help_page(WhichPage::Attributes, program_version)?;
    }

    if open_or_print.do_print() {
        let mut out = stdout().lock();
        writeln!(
            &mut out,
            "List of the valid attributes and details about them:\n\n\
             (Legend:\n \
             need: whether a value is required for the attribute.\n \
             kind: whether a single value is expected or a list, with how the text is parsed.\n \
             autolink: yes means, automatically link what looks like URLs.\n \
             indexing: whether the value(s) is/are indexed, and how.\n\
             )\n"
        )?;

        for att in METADATA_SPECIFICATION {
            writeln!(&mut out, "{}", att)?;
        }
    }

    Ok(())
}
