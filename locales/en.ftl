locale-name = English

-org-name = OpenStax Poland

-brand-name = Adaptarr!



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
    ["user:password:bad-confirmation"] Password don't match
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
    ["user:password:bad-confirmation"] Password don't match
    ["password:reset:invalid"] Password reset code is not valid
    ["password:reset:passwords-dont-match"] Password don't match
    ["user:change-password:empty"] Password cannot be empty
   *[other] Unknown error occurred: { $code }
}



## Mail template

-mail-url = <a href="{ $url }" target="_blank" rel="noopener">{ $text }</a>

mail-logo-alt = { -org-name }™ logo

mail-footer = This message was auto-generated, please do not respond to it.
    You are receiving it because you have an { -brand-name } account.



## Invitation email

mail-invite-subject = Invitation

# Variables:
# - $url (string): registration URL
mail-invite-text =
    You have been invited to join { -brand-name }, { -org-name }'s service
    for book translators.

    To complete you registration please go to following URL

        { $url }

mail-invite-before-button =
    You have been invited to join { -brand-name }, { -org-name }'s service
    for book translators.

    To complete you registration please click the button below

mail-invite-register-button = Register here

# Variables:
# - $url (string): registration URL
mail-invite-after-button =
    Or copy the following URL into your address bar:
    { -mail-url(url: $url, text: $url) }

# Variables:
# - $email (string): invitee's email address
mail-invite-footer = You are receiving this message because a member of
    { -org-name } has invited { $email } to join { -brand-name }.



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
    { -mail-url(url: $url, text: $url) }

    If you have not requested a password reset you don't have to
    do anything, your account is still secure.



## Notification email
#
# Notification emails are divided into section. Each section begins with
# mail-notify-group-header-KIND, where KIND is the type of events in this
# section. Each section then contains a list of events, formatted with
# mail-notify-event-KIND.

mail-notify-subject = Information on progress of work

mail-notify-footer =
    Thank you for participating in out project.

    Sincerely 
    The { -org-name } team

# Header displayed before notifications about module assignment.
mail-notify-group-header-assigned = Information on assignment of modules:

# Notification about a module being assigned to a user.
#
# Variables:
# - $actorname (string): name of the user who assigned the module
# - $actorurl (string): URL to profile of the user who assigned the module
# - $moduletitle (string): title of the module which was assigned
# - $moduleurl (string): URL to the module which was assigned
# - $bookcount (number): Number of books in which the module is used
# - $booktitle (string): Title of one of books in which the module is used
# - $bookurl (string): URL to the book $booktitle
mail-notify-event-assigned-text =
    { $actorname } assigned you the module “{ $moduletitle }” ({
    $moduleurl }). { $bookcount ->
        [0] This module is not used in any books.
        [1] This module is used in book “{ $booktitle }” ({ $bookurl }).
       *[other] This module is used in { $bookcount } books, including “{
            $booktitle }” ({ $bookurl }).
    }
mail-notify-event-assigned =
    { -mail-url(url: $actorurl, text: $actorname) } assigned you the module
    { -mail-url(url: $moduleurl, text: $moduletitle) }. { $bookcount ->
        [0] This module is not used in any books.
        [1] This module is used in book {
            -mail-url(url: $bookurl, text: $booktitle) }.
       *[other] This module is used in { $bookcount } books, including
            { -mail-url(url: $bookurl, text: $booktitle) }.
    }

# Header displayed before notifications about editing process finishing for
# drafts.
mail-notify-group-header-process-ended =
    Information on conclusion of editing works:

# Notification about an editing process being finished for a draft.
#
# Variables:
# - $moduletitle (string): title of the module for whose draft the process ended
# - $moduleurl (string): URL to the module $moduletitle
mail-notify-event-process-ended-text =
    We are happy to inform that work on module “{ $moduletitle }” ({
    $moduleurl }) has successfully concluded.
mail-notify-event-process-ended =
    We are happy to inform that work on module {
        -mail-url(url: $moduleurl, text: $moduletitle)
    } has successfully concluded.

# Header displayed before notifications about user being assigned to a slot in
# an editing process.
mail-notify-group-header-slot-filled =
    Information on assignment of work:

# Notification about user being assigned to a slot (or slots) in an editing
# process for a draft.
#
# Variables:
# - $moduletitle (string): title of the module in which the user was assigned
# - $moduleurl (string): URL to the module $moduletitle
# - $slotname (string): name of the slot to which the user was assigned
mail-notify-event-slot-filled-text =
    You have been assigned the role of { $slotname } for module “{ $moduletitle
    }” ({ $moduleurl }).
mail-notify-event-slot-filled =
    You have been assigned the role of { $slotname } for module {
    -mail-url(url: $moduleurl, text: $moduletitle) }.

-mail-notify-unknown-text =
    You can see { $count ->
        [1] it
       *[other] them
    } in the notification centre ({ $url }).
-mail-notify-unknown =
    You can see { $count ->
        [1] it
       *[other] them
    } in the { -mail-url(url: $url, text: "notification centre") }.

# Message displayed at the end of the email if in there were unknown
# notifications in addition to normal notifications.
#
# Variables:
# - $count (number): Number of unknown notifications
# - $notification_centre_url (string): URL of the notifications centre
mail-notify-also-unknown-events-text =
    And { $count ->
        [1] one other event
       *[other] { $count } other events
    } which we could not represent in this email. {
        -mail-notify-unknown-text(count: $count, url: $notification_centre_url) }
mail-notify-also-unknown-events =
    And { $count ->
        [1] one other event
       *[other] { $count } other events
    } which we could not represent in this email.
    { -mail-notify-unknown(count: $count, url: $notification_centre_url) }

# Message displayed at the end of the email if in there were only unknown
# notifications.
#
# Variables:
# - $count (number): Number of unknown notifications
# - $notification_centre_url (string): URL of the notifications centre
mail-notify-only-unknown-events-text =
    We want to inform you of { $count ->
        [1] one new event
       *[other] { $count } new events
    } which we could not represent in this email. {
        -mail-notify-unknown-text(count: $count, url: $notification_centre_url) }
mail-notify-only-unknown-events =
    We want to inform you of { $count ->
        [1] one new event
       *[other] { $count } new events
    } which we could not represent in this email.
    { -mail-notify-unknown(count: $count, url: $notification_centre_url) }
