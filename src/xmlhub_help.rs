//! Help and other pages generated interactively (the system in that
//! is not yet well thought out)

use std::{io::Write, path::Path};

use ahtml::{flat::Flat, HtmlAllocator, Node};
use anyhow::Result;

use crate::util::with_output_to_file;

pub const CSS_CODE_BACKGROUND_COLOR: &str = "#f4f2e6";

fn standalone_html_styles() -> String {
    [
        r#"
    body {
      font-family: sans;
      margin: 0 auto;
      padding-left: 50px;
      padding-right: 50px;
      padding-top: 50px;
      padding-bottom: 50px;
      hyphens: auto;
      overflow-wrap: break-word;
      text-rendering: optimizeLegibility;
      font-kerning: normal;
    }
    h1 {
      font-family: serif;
    }
    h2, h3, h4, h5, h6 {
      font-family: serif;
    }
    h1, h2, h3, h4, h5, h6 {
      color: #104060;
      margin-top: 1.4em;
    }
    blockquote {
      margin: 1em 0 1em 1.7em;
      padding-left: 1em;
      border-left: 2px solid #e6e6e6;
      color: #606060;
    }
    code {
      font-family: Menlo, Monaco, "Lucida Console", Consolas, monospace;
      font-size: 85%;
      margin: 1px;
      padding: 1px;
      background-color: "#,
        CSS_CODE_BACKGROUND_COLOR,
        ";
    }
",
    ]
    .join("")
}

pub fn print_basic_standalone_html_page(
    title: &str,
    body: Flat<Node>,
    html: &HtmlAllocator,
    mut output: &mut dyn Write,
) -> Result<()> {
    let doc = html.html(
        [],
        [
            html.head(
                [],
                [
                    html.title([], html.text(title)?)?,
                    html.style([], html.text(standalone_html_styles())?)?,
                ],
            )?,
            html.body([], body)?,
        ],
    )?;
    html.print_html_document(doc, &mut output)?;
    Ok(())
}

pub fn save_basic_standalone_html_page(
    output_path: &Path,
    title: &str,
    body: Flat<Node>,
    html: &HtmlAllocator,
) -> Result<()> {
    with_output_to_file(output_path, |output| -> Result<()> {
        Ok(print_basic_standalone_html_page(
            title, body, &html, output,
        )?)
    })
}
