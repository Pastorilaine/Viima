//! UI language for Rust-side strings (native menus, default tab titles, errors).
//! Web pages translate themselves with `assets/i18n.js`, keyed off `<html lang>`.
use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn set_language(lang: &str) {
    ENGLISH.store(lang == "en", Ordering::Relaxed);
}

pub fn lang() -> &'static str {
    if ENGLISH.load(Ordering::Relaxed) { "en" } else { "fi" }
}

/// Picks the Finnish or English variant of a string.
pub fn tr(fi: &'static str, en: &'static str) -> &'static str {
    if ENGLISH.load(Ordering::Relaxed) { en } else { fi }
}

pub fn new_tab_title() -> &'static str {
    tr("Uusi välilehti", "New tab")
}
