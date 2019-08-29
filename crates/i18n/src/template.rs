use fluent_bundle::types::FluentValue;
use log::warn;
use serde::Serialize;
use std::{borrow::Cow, cell::Cell, collections::HashMap};
use tera::{Tera, Value, compile_templates};

use crate::Locale;

pub use tera::Error as RenderError;

thread_local!(static LOCALE: Cell<Option<&'static Locale>> = Cell::new(None));

pub struct LocalizedTera(Tera);

impl LocalizedTera {
    pub fn new(glob: &str) -> LocalizedTera {
        let mut tera = compile_templates!(glob);
        tera.register_function("_", Box::new(translate_fun));
        tera.register_filter("translate", translate_filter);
        LocalizedTera(tera)
    }

    pub fn render_i18n<T>(&self, name: &str, data: &T, locale: &'static Locale)
    -> Result<String, RenderError>
    where
        T: Serialize,
    {
        LOCALE.with(|loc| loc.set(Some(locale)));
        self.render(name, data).map_err(From::from)
    }
}

impl std::ops::Deref for LocalizedTera {
    type Target = Tera;

    fn deref(&self) -> &Tera {
        &self.0
    }
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
                    return Err(RenderError::from(format!(
                        "Arguments to _() can only be numbers or strings, \
                        but `{}` was {:?}",
                        key,
                        value,
                    ))),
                Value::Number(n) => FluentValue::Number(Cow::from(n.to_string())),
                Value::String(s) => FluentValue::String(Cow::from(s)),
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

#[macro_export]
macro_rules! localized_templates {
    (pub $name:ident = $path:expr) => {
        lazy_static::lazy_static! {
            pub static ref $name: $crate::LocalizedTera =
                $crate::LocalizedTera::new($path);
        }
    };
    ($name:ident = $path:expr) => {
        lazy_static::lazy_static! {
            static ref $name: $crate::LocalizedTera =
                $crate::LocalizedTera::new($path);
        }
    };
}
