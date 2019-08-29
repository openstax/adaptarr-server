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

/// Arguments for `mail/notify`.
#[derive(Serialize)]
pub struct NotifyMailArgs<'a> {
    /// List of new events to include in the email.
    pub events: &'a [(crate::events::Group, Vec<crate::events::ExpandedEvent>)],
    // /// Various URLs which can be used in the email.
    pub urls: NotifyMailArgsUrls<'a>,
}

#[derive(Serialize)]
pub struct NotifyMailArgsUrls<'a> {
    pub notification_centre: Cow<'a, str>,
}
