locale-name = English

## Login page

login-field-email = E-Mail address

login-field-password = Password

login-reset-password = Reset password

# Variables:
# - $code (string): error code
login-error = { $code ->
    ["user:not-found"] User not found
    ["user:authenticate:bad-password"] Bad password
   *[other] Unknown error occurred: { $code }
}

## Session elevation page

elevate-entering-superuser-mode =
    <p>You're entering superuser mode</p>
    <p>We won't ask for your password again in the next 15 minutes</p>

elevate-field-password = Password

elevate-submit = Authorize

# Variables:
# - $code (string): error code
elevate-error = { $code ->
    ["user:authenticate:bad-password"] Bad password
   *[other] Unknown error occurred: { $code }
}

## Logout page

logout-message = <p>You have been logged out.</p>

## Registration page

register-field-name = Name

register-field-password = Password

register-field-repeat-password = Password

register-submit = Register

# Variables:
# - $code (string): error code
register-error = { $code ->
    ["user:register:passwords-dont-match"] Password don't match
    ["user:register:email-changed"]
        You can't change your email during registration
    ["invitation:invalid"] Invitation code is not valid
    ["user:new:empty-name"] Name cannot be empty
    ["user:new:empty-password"] Password cannot be empty
   *[other] Unknown error occurred: { $code }
}

## Password reset page

reset-field-password = Password

reset-field-repeat-password = Password

reset-field-email = E-Mail address

reset-message =
    <p>Please enter your email address and we will mail you further
    instructions.</p>

reset-message-sent = <p>Instructions have been sent.</p>

reset-submit = Reset password

# Variables:
# - $code (string): error code
reset-error = { $code ->
    ["user:not-found"] Unknown email address
    ["password:reset:invalid"] Password reset code is not valid
    ["password:reset:passwords-dont-match"] Password don't match
    ["user:change-password:empty"] Password cannot be empty
   *[other] Unknown error occurred: { $code }
}

## Mail template

mail-logo-alt = OpenStax Polska™ logo

mail-footer =
    You are receiving this email because you are a member of Adaptarr!.

## Invitation email

mail-invite-subject = Invitation

# Variables:
# - $url (string): registration URL
mail-invite-text =
    You have been invited to join Adaptarr!, Katalyst Education's service
    for book translators.

    To complete you registration please go to following URL

        { $url }

mail-invite-before-button =
    You have been invited to join Adaptarr!, Katalyst Education's service
    for book translators.

    To complete you registration please click the button below

mail-invite-register-button = Register here

# Variables:
# - $url (string): registration URL
mail-invite-after-button =
    Or copy the following URL into your address bar:
    <a href="{ $url }" target="_blank" rel="noopener">{ $url }</a>

# Variables:
# - $email (string): invitee's email address
mail-invite-footer = You are receiving this message because someone has invited
    { $email } to join Adaptarr!.

## Password reset email

mail-reset-subject = Password reset

# Variables:
# - $username (string): user's name
# - $url (string): password reset URL
mail-reset-text =
    Hello, { $username }.

    To reset your password please go to the following URL

        { $url }

    If you have not requested a password reset you don't have to do anything,
    your account is still secure.

# Variables:
# - $username (string): user's name
mail-reset-before-button =
    Hello, { $username }

    To reset your password place click the link bellow

mail-reset-button = Reset password

# Variables:
# - $url (string): password reset URL
mail-reset-after-button =
    Or enter this URL into your browser's address bar:
    <a href="{ $url }" target="_blank" rel="noopener">{ $url }</a>

    If you have not requested a password reset you don't have to
    do anything, your account is still secure.
