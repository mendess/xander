use std::{borrow::Borrow, fmt::Display, ops::Deref};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct CardName(String);

impl CardName {
    pub fn trimming_double_faced(self) -> Self {
        trim_if_double_faced(&self.0)
            .map(|s| Self(s.to_owned()))
            .unwrap_or(self)
    }

    pub fn as_slice(&self) -> &CName {
        self.0.as_str().into()
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CName(str);

impl CName {
    pub fn trimming_double_faced(&self) -> &Self {
        trim_if_double_faced(&self.0)
            .map(Into::into)
            .unwrap_or(self)
    }
}

impl From<String> for CardName {
    fn from(value: String) -> Self {
        Self(
            fix_lotr_accented_cards(&value)
                .map(ToOwned::to_owned)
                .unwrap_or(value),
        )
    }
}

impl From<&str> for &CName {
    fn from(value: &str) -> Self {
        let value = fix_lotr_accented_cards(value).unwrap_or(value);
        unsafe { std::mem::transmute(value) }
    }
}

impl Display for CardName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for CName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Deref for CardName {
    type Target = CName;

    fn deref(&self) -> &Self::Target {
        self.0.as_str().into()
    }
}

impl Deref for CName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<CName> for CardName {
    fn borrow(&self) -> &CName {
        self.0.as_str().into()
    }
}

impl ToOwned for CName {
    type Owned = CardName;
    fn to_owned(&self) -> Self::Owned {
        CardName(self.0.to_owned())
    }
}

fn trim_if_double_faced(card: &str) -> Option<&str> {
    card.char_indices()
        .find(|(_, c)| *c == '/')
        .map(|(idx, _)| card[..idx].trim())
}

fn fix_lotr_accented_cards(card: &str) -> Option<&'static str> {
    match card {
        "Lorien Revealed" => "Lórien Revealed".into(),
        "Troll of Khazad-dum" => "Troll of Khazad-dûm".into(),
        _ => None,
    }
}
