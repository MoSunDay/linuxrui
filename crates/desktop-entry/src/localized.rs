//! Localized values and locale fallback (`Name[zh_CN]` etc.).

use std::collections::BTreeMap;

/// Locale-keyed string with an unlocalized default, e.g. `Name` +
/// `Name[zh_CN]`. Lookup order: exact locale, locale without `@modifier`,
/// language only, then the default.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalizedString {
    default: Option<String>,
    locales: BTreeMap<String, String>,
}

impl LocalizedString {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_default(value: impl Into<String>) -> Self {
        Self {
            default: Some(value.into()),
            locales: BTreeMap::new(),
        }
    }

    pub fn set_default(&mut self, value: String) {
        self.default = Some(value);
    }

    pub fn set_locale(&mut self, locale: String, value: String) {
        self.locales.insert(locale, value);
    }

    pub fn default_value(&self) -> Option<&str> {
        self.default.as_deref()
    }

    pub fn locale_value(&self, locale: &str) -> Option<&str> {
        self.locales.get(locale).map(|s| s.as_str())
    }

    /// Resolve with the spec's fallback chain. `None` if nothing is set.
    pub fn get(&self, locale: Option<&str>) -> Option<&str> {
        if let Some(tag) = locale {
            for candidate in locale_candidates(tag) {
                if let Some(hit) = self.locales.get(candidate.as_str()) {
                    return Some(hit.as_str());
                }
            }
        }
        self.default.as_deref()
    }

    /// True when neither a default nor any locale value exists.
    pub fn is_empty(&self) -> bool {
        self.default.is_none() && self.locales.is_empty()
    }
}

/// Locale-keyed `;`-separated string list (e.g. `Keywords`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalizedStringList {
    default: Option<Vec<String>>,
    locales: BTreeMap<String, Vec<String>>,
}

impl LocalizedStringList {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn set_default(&mut self, values: Vec<String>) {
        self.default = Some(values);
    }

    pub fn set_locale(&mut self, locale: String, values: Vec<String>) {
        self.locales.insert(locale, values);
    }

    pub fn get(&self, locale: Option<&str>) -> &[String] {
        if let Some(tag) = locale {
            for candidate in locale_candidates(tag) {
                if let Some(hit) = self.locales.get(candidate.as_str()) {
                    return hit.as_slice();
                }
            }
        }
        self.default.as_deref().unwrap_or(&[])
    }
}

/// Candidate lookup order for one requested locale:
/// `zh_CN@pinyin` -> `zh_CN` -> `zh`, and encoding suffixes like
/// `zh_CN.UTF-8` -> `zh_CN` are stripped as well.
fn locale_candidates(tag: &str) -> Vec<String> {
    let mut out = vec![tag.to_string()];
    let mut base = tag.to_string();
    let push_if_new = |out: &mut Vec<String>, base: &str| {
        if !base.is_empty() && !out.iter().any(|c| c == base) {
            out.push(base.to_string());
        }
    };
    if let Some(at) = base.find('@') {
        base.truncate(at);
    }
    push_if_new(&mut out, &base);
    if let Some(dot) = base.find('.') {
        base.truncate(dot);
    }
    push_if_new(&mut out, &base);
    if let Some(underscore) = base.find('_') {
        base.truncate(underscore);
    }
    push_if_new(&mut out, &base);
    out
}

/// Split a spec string list: `;`-separated, terminated by a trailing `;`.
/// `\;` escapes a literal semicolon.
pub fn split_string_list(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&';') => {
                chars.next();
                current.push(';');
            }
            ';' => {
                if !current.is_empty() {
                    items.push(std::mem::take(&mut current));
                }
            }
            other => current.push(other),
        }
    }
    if !current.is_empty() {
        items.push(current);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_chain_exact_then_lang_then_default() {
        let mut s = LocalizedString::empty();
        s.set_default("File Manager".into());
        s.set_locale("zh".into(), "文件管理器".into());
        // zh_CN has no exact entry, so it falls back to the zh language entry.
        assert_eq!(s.get(Some("zh_CN")), Some("文件管理器"));
        assert_eq!(s.get(Some("zh")), Some("文件管理器"));
        assert_eq!(s.get(None), Some("File Manager"));
        assert_eq!(s.get(Some("fr")), Some("File Manager"));
    }

    #[test]
    fn encoding_suffix_is_stripped() {
        let mut s = LocalizedString::empty();
        s.set_default("Editor".into());
        s.set_locale("zh_CN".into(), "编辑器".into());
        assert_eq!(s.get(Some("zh_CN.UTF-8")), Some("编辑器"));
        assert_eq!(s.get(Some("zh")), Some("Editor"));
    }

    #[test]
    fn modifier_and_country_are_stripped() {
        let mut s = LocalizedString::empty();
        s.set_default("Editor".into());
        s.set_locale("zh_CN".into(), "编辑器".into());
        assert_eq!(s.get(Some("zh_CN@pinyin")), Some("编辑器"));
        assert_eq!(s.get(Some("zh_CN")), Some("编辑器"));
        assert_eq!(s.get(Some("zh")), Some("Editor"));
        assert_eq!(s.get(Some("en")), Some("Editor"));
    }

    #[test]
    fn string_list_splits_and_unescapes() {
        assert_eq!(split_string_list("a;b;c;"), vec!["a", "b", "c"]);
        assert_eq!(split_string_list("a\\;b;c"), vec!["a;b", "c"]);
        assert_eq!(split_string_list(""), Vec::<String>::new());
    }
}
