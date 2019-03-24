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
   *[other] Unknown error occurred: { $code }
}
