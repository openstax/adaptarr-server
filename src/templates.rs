use fluent_bundle::types::FluentValue;
use tera::{Tera, Value};
use std::{cell::Cell, collections::HashMap};
use serde::Serialize;

use crate::{
    i18n::Locale,
    models::user::{PublicData as UserData},
};

thread_local!(static LOCALE: Cell<Option<&'static Locale<'static>>> = Cell::new(None));

lazy_static! {
    pub static ref PAGES: Tera = create("templates/pages/**/*");

    pub static ref MAILS: Tera = create("templates/mail/**/*");
}

pub trait LocalizedTera {
    fn render_i18n<T>(&self, name: &str, data: &T, locale: &'static Locale<'static>)
    -> tera::Result<String>
    where
        T: Serialize;
}

impl LocalizedTera for Tera {
    fn render_i18n<T>(&self, name: &str, data: &T, locale: &'static Locale<'static>)
    -> tera::Result<String>
    where
        T: Serialize,
    {
        LOCALE.with(|loc| loc.set(Some(locale)));
        self.render(name, data)
    }
}

fn create(glob: &str) -> Tera {
    let mut tera = compile_templates!(glob);
    tera.register_function("_", Box::new(translate_fun));
    tera.register_filter("translate", translate_filter);
    tera
}

fn translate_fun(args: HashMap<String, Value>) -> tera::Result<Value> {
    if let Some(message) = translate(args)? {
        Ok(message)
    } else {
        Ok(Value::Null)
    }
}

fn translate_filter(default: Value, args: HashMap<String, Value>)
-> tera::Result<Value> {
    if let Some(message) = translate(args)? {
        Ok(message)
    } else {
        Ok(default)
    }
}

fn translate(mut args: HashMap<String, Value>) -> tera::Result<Option<Value>> {
    let locale = LOCALE.with(|locale| {
        locale.get().expect("_() invoked without a locale")
    });

    let key = args.remove("key")
        .ok_or_else(|| "argument `key` of _() is mandatory")?;
    let key = key.as_str()
        .ok_or_else(|| "argument `key` of _() must be a string")?;

    let args = args.iter()
        .map(|(key, value)| {
            let value = match value {
                Value::Null | Value::Bool(_) | Value::Array(_) |
                Value::Object(_) =>
                    return Err(tera::Error::from(format!(
                        "Arguments to _() can only be numbers or strings, \
                        but `{}` was {:?}",
                        key,
                        value,
                    ))),
                Value::Number(n) => FluentValue::Number(n.to_string()),
                Value::String(s) => FluentValue::String(s.clone()),
            };
            Ok((key.as_str(), value))
        })
        .collect::<tera::Result<_>>()?;

    if let Some(value) = locale.format(key, &args) {
        Ok(Some(value.into()))
    } else {
        warn!("Message {} missing from locale {}", key, locale.code);
        Ok(None)
    }
}

/// Arguments for `mail/invite`.
#[derive(Serialize)]
pub struct InviteMailArgs<'a> {
    /// Registration URL.
    pub url: &'a str,
    /// Email address which was invited.
    pub email: &'a str,
}

/// Arguments for `mail/reset`.
#[derive(Serialize)]
pub struct ResetMailArgs<'a> {
    /// User to whom the email is sent.
    pub user: UserData,
    /// Password reset URL.
    pub url: &'a str,
}
