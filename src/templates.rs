use tera::Tera;

lazy_static! {
    pub static ref PAGES: Tera = {
        compile_templates!("templates/pages/**/*")
    };
}

lazy_static! {
    pub static ref MAILS: Tera = {
        compile_templates!("templates/mail/**/*")
    };
}
