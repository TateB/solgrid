use serde::Deserialize;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NatspecCommentStyle {
    #[default]
    TripleSlash,
    Either,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NatspecContinuationIndent {
    #[default]
    Padded,
    None,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NatspecTagSettings {
    pub title: NatspecTagRuleConfig,
    pub author: NatspecTagRuleConfig,
    pub notice: NatspecTagRuleConfig,
    pub dev: NatspecTagRuleConfig,
    pub param: NatspecTagRuleConfig,
    #[serde(rename = "return")]
    pub return_tag: NatspecTagRuleConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NatspecTagRuleConfig {
    pub enabled: Option<bool>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub skip_internal: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NatspecSettings {
    pub comment_style: NatspecCommentStyle,
    pub continuation_indent: NatspecContinuationIndent,
    pub tags: NatspecTagSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NoEmptyBlocksSettings {
    pub allow_comments: bool,
}

impl Default for NoEmptyBlocksSettings {
    fn default() -> Self {
        Self {
            allow_comments: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryHeaderId {
    Types,
    ConstantsAndImmutables,
    Constants,
    Immutables,
    Storage,
    Events,
    Errors,
    Modifiers,
    Initialization,
    Functions,
    Implementation,
    InternalFunctions,
    PrivateFunctions,
}

impl CategoryHeaderId {
    pub const ALL: [Self; 13] = [
        Self::Types,
        Self::ConstantsAndImmutables,
        Self::Constants,
        Self::Immutables,
        Self::Storage,
        Self::Events,
        Self::Errors,
        Self::Modifiers,
        Self::Initialization,
        Self::Functions,
        Self::Implementation,
        Self::InternalFunctions,
        Self::PrivateFunctions,
    ];

    pub fn default_label(self) -> &'static str {
        match self {
            Self::Types => "Types",
            Self::ConstantsAndImmutables => "Constants & Immutables",
            Self::Constants => "Constants",
            Self::Immutables => "Immutables",
            Self::Storage => "Storage",
            Self::Events => "Events",
            Self::Errors => "Errors",
            Self::Modifiers => "Modifiers",
            Self::Initialization => "Initialization",
            Self::Functions => "Functions",
            Self::Implementation => "Implementation",
            Self::InternalFunctions => "Internal Functions",
            Self::PrivateFunctions => "Private Functions",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CategoryHeaderLabels {
    pub types: Option<String>,
    pub constants_and_immutables: Option<String>,
    pub constants: Option<String>,
    pub immutables: Option<String>,
    pub storage: Option<String>,
    pub events: Option<String>,
    pub errors: Option<String>,
    pub modifiers: Option<String>,
    pub initialization: Option<String>,
    pub functions: Option<String>,
    pub implementation: Option<String>,
    pub internal_functions: Option<String>,
    pub private_functions: Option<String>,
}

impl CategoryHeaderLabels {
    pub fn label_for(&self, id: CategoryHeaderId) -> Cow<'_, str> {
        match id {
            CategoryHeaderId::Types => self
                .types
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::ConstantsAndImmutables => self
                .constants_and_immutables
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Constants => self
                .constants
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Immutables => self
                .immutables
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Storage => self
                .storage
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Events => self
                .events
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Errors => self
                .errors
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Modifiers => self
                .modifiers
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Initialization => self
                .initialization
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Functions => self
                .functions
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::Implementation => self
                .implementation
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::InternalFunctions => self
                .internal_functions
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
            CategoryHeaderId::PrivateFunctions => self
                .private_functions
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(id.default_label())),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CategoryHeadersSettings {
    pub min_categories: usize,
    pub initialization_functions: Vec<String>,
    pub order: Vec<CategoryHeaderId>,
    pub labels: CategoryHeaderLabels,
}

impl Default for CategoryHeadersSettings {
    fn default() -> Self {
        Self {
            min_categories: 2,
            initialization_functions: Vec::new(),
            order: Vec::new(),
            labels: CategoryHeaderLabels::default(),
        }
    }
}

impl CategoryHeadersSettings {
    pub fn ordered_categories(&self) -> Vec<CategoryHeaderId> {
        if self.order.is_empty() {
            CategoryHeaderId::ALL.to_vec()
        } else {
            self.order.clone()
        }
    }

    pub fn display_categories(&self) -> Vec<CategoryHeaderId> {
        let mut ordered = self.ordered_categories();
        for id in CategoryHeaderId::ALL {
            if !ordered.contains(&id) {
                ordered.push(id);
            }
        }
        ordered
    }

    pub fn label_for(&self, id: CategoryHeaderId) -> Cow<'_, str> {
        self.labels.label_for(id)
    }

    pub fn category_for_label(&self, label: &str) -> Option<CategoryHeaderId> {
        self.display_categories()
            .into_iter()
            .find(|id| self.label_for(*id).as_ref() == label)
    }

    pub fn validate(&self) -> Result<(), String> {
        let ordered = self.ordered_categories();
        let mut seen_ids = std::collections::HashSet::new();
        for id in &ordered {
            if !seen_ids.insert(*id) {
                return Err(format!("duplicate category id `{id:?}` in `order`"));
            }
        }

        let mut seen_labels = std::collections::HashSet::new();
        for id in self.display_categories() {
            let label = self.label_for(id);
            if !seen_labels.insert(label.to_string()) {
                return Err(format!("duplicate category label `{label}`"));
            }
        }

        Ok(())
    }

    pub fn prefers_split_constants_and_immutables(&self) -> bool {
        self.order.iter().any(|id| {
            matches!(
                id,
                CategoryHeaderId::Constants | CategoryHeaderId::Immutables
            )
        }) || self.labels.constants.is_some()
            || self.labels.immutables.is_some()
    }
}
