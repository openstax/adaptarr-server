/// Arguments for `mail/reset`.
#[derive(Serialize)]
pub struct ResetMailArgs<'a> {
    /// User to whom the email is sent.
    pub user: UserData,
    /// Password reset URL.
    pub url: &'a str,
}
