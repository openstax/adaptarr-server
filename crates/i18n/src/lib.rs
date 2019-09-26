use adaptarr_util::SingleInit;

mod language_tag;
mod locale;
mod template;

pub use self::{
    language_tag::{LanguageTag, LanguageRange, ParseLanguageTagError},
    locale::{I18n, Locale, LoadLocalesError},
    template::{LocalizedTera, RenderError},
};

static LOCALES: SingleInit<I18n> = SingleInit::uninit();

pub fn load() -> Result<I18n<'static>, LoadLocalesError> {
    LOCALES.get_or_try_init(I18n::load).map(Clone::clone)
}
