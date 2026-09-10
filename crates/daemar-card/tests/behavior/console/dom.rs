//! The parsed-DOM oracle: an HTML5 parse of each response, walked as a
//! tree so that assertions read what a browser would build — an escaped
//! `<script>` is a text node here and an element only if the server
//! failed (S3-B7). The console's markup contract lives in [`role`] and
//! [`field`]: every landmark is a `data-role`, every rendered value a
//! `data-field`, so the tests bind to meaning, not to layout.
//!
//! Provisional until the typed skeleton is approved; each name is used
//! from exactly one constant so a skeleton-time rename touches one site.

use html5ever::tendril::TendrilSink as _;
use html5ever::ParseOpts;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// `data-role` values: the landmarks of the screen.
pub(crate) mod role {
    pub(crate) const QUEUE: &str = "queue";
    pub(crate) const QUEUE_ROW: &str = "queue-row";
    pub(crate) const QUEUE_EMPTY: &str = "queue-empty";
    pub(crate) const CARD_IDENTITY: &str = "card-identity";
    pub(crate) const STREAM: &str = "stream";
    pub(crate) const STREAM_ROW: &str = "stream-row";
    /// The link inside a stream row that opens the entry's inspector.
    pub(crate) const INSPECT: &str = "inspect";
    pub(crate) const INSPECTOR: &str = "inspector";
    /// The inspector's verbatim payload JSON.
    pub(crate) const PAYLOAD: &str = "payload";
    pub(crate) const ERROR: &str = "error";
}

/// `data-field` values: one per rendered value.
pub(crate) mod field {
    pub(crate) const CARD_ID: &str = "card-id";
    pub(crate) const TITLE: &str = "title";
    pub(crate) const TASK_KEY: &str = "task-key";
    pub(crate) const WORKSPACE: &str = "workspace";
    pub(crate) const CREATED_AT: &str = "created-at";
    pub(crate) const LAST_ACTIVITY: &str = "last-activity";
    pub(crate) const SEQUENCE: &str = "sequence";
    pub(crate) const ENTRY_ID: &str = "entry-id";
    pub(crate) const ENTRY_TYPE: &str = "entry-type";
    pub(crate) const SCHEMA_VERSION: &str = "schema-version";
    pub(crate) const PRODUCER_ID: &str = "producer-id";
    pub(crate) const PRODUCER_KIND: &str = "producer-kind";
    pub(crate) const RECORDED_AT: &str = "recorded-at";
    pub(crate) const SUMMARY: &str = "summary";
    pub(crate) const REASON: &str = "reason";
    pub(crate) const STAGE: &str = "stage";
    /// The per-row epistemic label (S3-B5): always "reported".
    pub(crate) const PROVENANCE: &str = "provenance";
}

/// Attribute names of the markup contract.
pub(crate) const ROLE_ATTR: &str = "data-role";
pub(crate) const FIELD_ATTR: &str = "data-field";
pub(crate) const CARD_ID_ATTR: &str = "data-card-id";
pub(crate) const SEQUENCE_ATTR: &str = "data-sequence";
pub(crate) const ENTRY_ID_ATTR: &str = "data-entry-id";
/// The selected queue row carries `aria-current="page"`.
pub(crate) const SELECTED_ATTR: &str = "aria-current";
pub(crate) const SELECTED_VALUE: &str = "page";

/// A parsed page.
#[derive(Debug)]
pub(crate) struct Document {
    root: Handle,
}

/// One element of a parsed page.
#[derive(Debug, Clone)]
pub(crate) struct El(Handle);

impl Document {
    pub(crate) fn parse(html: &str) -> Self {
        let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default()).one(html);
        Self { root: dom.document }
    }

    /// Every element in document order.
    pub(crate) fn elements(&self) -> Vec<El> {
        let mut out = Vec::new();
        collect_elements(&self.root, &mut out);
        out
    }

    pub(crate) fn by_role(&self, role: &str) -> Vec<El> {
        self.elements()
            .into_iter()
            .filter(|el| el.attr(ROLE_ATTR).as_deref() == Some(role))
            .collect()
    }

    /// The single element with this role, or a panic naming what was found.
    pub(crate) fn one_by_role(&self, role: &str) -> El {
        let mut found = self.by_role(role);
        assert_eq!(
            found.len(),
            1,
            "expected exactly one data-role={role}, found {}",
            found.len()
        );
        found.pop().expect("one element")
    }

    pub(crate) fn has_role(&self, role: &str) -> bool {
        !self.by_role(role).is_empty()
    }

    pub(crate) fn by_tag(&self, tag: &str) -> Vec<El> {
        self.elements()
            .into_iter()
            .filter(|el| el.tag() == tag)
            .collect()
    }

    /// All text of the page, concatenated in document order.
    pub(crate) fn text(&self) -> String {
        let mut out = String::new();
        collect_text(&self.root, &mut out);
        out
    }
}

impl El {
    pub(crate) fn tag(&self) -> String {
        match &self.0.data {
            NodeData::Element { name, .. } => name.local.to_string(),
            NodeData::Document
            | NodeData::Doctype { .. }
            | NodeData::Text { .. }
            | NodeData::Comment { .. }
            | NodeData::ProcessingInstruction { .. } => String::new(),
        }
    }

    pub(crate) fn attrs(&self) -> Vec<(String, String)> {
        match &self.0.data {
            NodeData::Element { attrs, .. } => attrs
                .borrow()
                .iter()
                .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                .collect(),
            NodeData::Document
            | NodeData::Doctype { .. }
            | NodeData::Text { .. }
            | NodeData::Comment { .. }
            | NodeData::ProcessingInstruction { .. } => Vec::new(),
        }
    }

    pub(crate) fn attr(&self, name: &str) -> Option<String> {
        self.attrs()
            .into_iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    /// Descendant elements in document order (self excluded).
    pub(crate) fn descendants(&self) -> Vec<El> {
        let mut out = Vec::new();
        for child in self.0.children.borrow().iter() {
            collect_elements(child, &mut out);
        }
        out
    }

    pub(crate) fn descendants_by_role(&self, role: &str) -> Vec<El> {
        self.descendants()
            .into_iter()
            .filter(|el| el.attr(ROLE_ATTR).as_deref() == Some(role))
            .collect()
    }

    /// The descendant carrying `data-field=name`, if rendered. Exactly one
    /// may exist: two spellings of one fact is a fabrication, not a
    /// rendering. Whether the one is visible is a browser question and is
    /// proved in the browser layer.
    pub(crate) fn field(&self, name: &str) -> Option<El> {
        let mut found: Vec<El> = self
            .descendants()
            .into_iter()
            .filter(|el| el.attr(FIELD_ATTR).as_deref() == Some(name))
            .collect();
        assert!(
            found.len() <= 1,
            "data-field={name} rendered {} times inside <{}>",
            found.len(),
            self.tag()
        );
        found.pop()
    }

    /// The rendered value of a field: a `<time datetime>` reads its
    /// machine value, everything else its trimmed text.
    pub(crate) fn value(&self) -> String {
        if self.tag() == "time" {
            if let Some(machine) = self.attr("datetime") {
                return machine;
            }
        }
        self.text().trim().to_owned()
    }

    /// The trimmed rendered value of a named field; panics if absent.
    pub(crate) fn field_value(&self, name: &str) -> String {
        self.field(name)
            .unwrap_or_else(|| panic!("no data-field={name} inside <{}>", self.tag()))
            .value()
    }

    /// Concatenated text of all descendant text nodes.
    pub(crate) fn text(&self) -> String {
        let mut out = String::new();
        collect_text(&self.0, &mut out);
        out
    }

    pub(crate) fn links(&self) -> Vec<String> {
        self.descendants()
            .into_iter()
            .filter(|el| el.tag() == "a")
            .filter_map(|el| el.attr("href"))
            .collect()
    }
}

fn collect_elements(node: &Handle, out: &mut Vec<El>) {
    if matches!(node.data, NodeData::Element { .. }) {
        out.push(El(node.clone()));
    }
    for child in node.children.borrow().iter() {
        collect_elements(child, out);
    }
}

fn collect_text(node: &Handle, out: &mut String) {
    if let NodeData::Text { contents } = &node.data {
        out.push_str(&contents.borrow());
    }
    for child in node.children.borrow().iter() {
        collect_text(child, out);
    }
}
