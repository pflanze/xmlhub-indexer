use std::{borrow::Cow, collections::HashMap, io::Write, sync::Arc};

use ahtml::{att, AId, HtmlAllocator, Node};
use anyhow::Result;

use crate::html_util::anchor;

#[derive(Debug, Clone)]
pub struct HintId<'id> {
    id: u32,
    hints_id: &'id str,
}

impl<'id> HintId<'id> {
    /// 1-based number
    fn to_num(&self) -> u32 {
        self.id
    }

    pub fn anchor_id(&self) -> String {
        let Self { id, hints_id } = self;
        format!("hints-{hints_id}-{id}")
    }

    /// For plain text, as ` [n]`
    pub fn to_plain(&self) -> String {
        format!(" [{}]", self.to_num())
    }

    /// To add after text, linking to the corresponding entry in
    /// `Hints::to_html`
    pub fn to_html(&self, html: &HtmlAllocator) -> Result<AId<Node>> {
        let num = self.to_num();
        html.sup(
            [],
            html.span(
                [],
                [
                    html.text(" [")?,
                    html.a(
                        [att("href", format!("#{}", self.anchor_id()))],
                        html.text(num.to_string())?,
                    )?,
                    html.text("]")?,
                ],
            )?,
        )
    }
}

/// Collect a number of hints, that are like footnotes: only record
/// the same hint text once, share its number.
pub struct Hints<'id> {
    id: &'id str,
    // Using Arc to avoid self-referencing issue--since we need to
    // mutate, too, can't handle via ouroboros, right?
    hints: Vec<Arc<Cow<'static, str>>>,
    index: HashMap<Arc<Cow<'static, str>>, u32>,
    active: bool,
}

impl<'id> Drop for Hints<'id> {
    fn drop(&mut self) {
        if self.active {
            panic!("`Hints` must not be dropped--call `to_html` or `print_plain` on it")
        }
    }
}

impl<'id> Hints<'id> {
    /// Each `id` must only be used once on a particular HTML page!
    pub fn new(id: &'id str) -> Self {
        Self {
            id,
            hints: Vec::new(),
            index: HashMap::new(),
            active: true,
        }
    }

    pub fn intern(&mut self, msg: Cow<'static, str>) -> HintId {
        if let Some(id) = self.index.get(&msg) {
            HintId {
                id: *id,
                hints_id: self.id,
            }
        } else {
            let msg = Arc::new(msg);
            self.hints.push(msg.clone());
            let hint = HintId {
                id: self
                    .hints
                    .len()
                    .try_into()
                    .expect("not generating more than u32::max different hints, OK?"),
                hints_id: self.id,
            };
            self.index.insert(msg, hint.id);
            hint
        }
    }

    pub fn to_html(mut self, html: &HtmlAllocator) -> Result<AId<Node>> {
        self.active = false;
        let mut items = html.new_vec();
        for (i, hint) in self.hints.iter().enumerate() {
            let s = &***hint;
            let id = HintId {
                id: (i + 1).try_into()?,
                hints_id: self.id,
            };
            items.push(html.li([], anchor(&id.anchor_id(), html.text(s)?, html)?)?)?;
        }
        html.ol([], items)
    }

    pub fn print_plain(mut self, mut out: impl Write) -> Result<()> {
        self.active = false;
        for (i, hint) in self.hints.iter().enumerate() {
            let id = HintId {
                id: (i + 1).try_into()?,
                hints_id: self.id,
            };
            let s = &***hint;
            writeln!(&mut out, "  {}. {s}", id.to_num())?;
        }
        Ok(())
    }
}
