locale-name = Polski

-org-name = OpenStax Poland

-brand-name = Adaptarr!



## Login page

login-title = Witaj w { -brand-name }

login-field-email = Adres e-mail

login-field-password = Hasło

login-reset-password = Zresetuj hasło

# Variables:
# - $code (string): error code
login-error = { $code ->
    ["user:not-found"] Nie znaleziono użytkownika
    ["user:authenticate:bad-password"] Nieprawidłowe hasło
   *[other] Wystąpił nieznany błąd: { $code }
}



## Session elevation page

elevate-entering-superuser-mode =
    <p>Wchodzisz w tryb super-użytkownika</p>
    <p>Nie będziemy pytać o twoje hasło ponownie przez następne 15 minut</p>

elevate-field-password = Hasło

elevate-submit = Autoryzuj

# Variables:
# - $code (string): error code
elevate-error = { $code ->
    ["user:authenticate:bad-password"] Nieprawidłowe hasło
   *[other] Wystąpił nieznany błąd: { $code }
}



## Logout page

logout-message = <p>Został/aś wylogowany/a.</p>



## Registration page

register-title = Utwórz konto

register-field-name = Imię

register-field-password = Hasło

register-field-repeat-password = Hasło

register-submit = Utwórz

# Variables:
# - $code (string): error code
register-error = { $code ->
    ["user:password:bad-confirmation"] Hasła nie pasują
    ["user:register:email-changed"]
        Nie możesz zmienić adresu e-mail podczas rejestracji
    ["invitation:invalid"] Nieprawidłowy kod zaproszenia
    ["user:new:empty-name"] Imię nie może być puste
    ["user:new:empty-password"] Hasło nie może być puste
   *[other] Wystąpił nieznany błąd: { $code }
}



## Password reset page

reset-field-password = Hasło

reset-field-repeat-password = Hasło

reset-field-email = Adres e-mail

reset-message =
    <p>Prosimy wpisać swój adres e-mail i kliknąć “zresetuj hasło”. Instrukcje
    jak zresetować hasło wyślemy na podany adres.</p>

reset-message-sent = <p>Instrukcje zostały wysłane</p>

reset-submit = Zresetuj hasło

# Variables:
# - $code (string): error code
reset-error = { $code ->
    ["user:not-found"] Nieznany adres e-mail
    ["user:password:bad-confirmation"] Hasała nie pasują
    ["password:reset:invalid"] Niepoprawny kod resetowania hasła
    ["password:reset:passwords-dont-match"] Hasała nie pasują
    ["user:change-password:empty"] Hasło nie może być puste
   *[other] Wystąpił nieznany błąd { $code }
}



## Mail template

-mail-url = <a href="{ $url }" target="_blank" rel="noopener">{ $text }</a>

mail-logo-alt = Logo { -org-name }™

mail-footer = Wiadomość została wygenerowana automatycznie, prosimy na nią
    nie odpowiadać. Otrzymujesz ją, ponieważ posiadasz konto w { -brand-name }.



## Invitation email

mail-invite-subject = Zaproszenie

# Variables:
# - $url (string): registration URL
mail-invite-text =
    Zostałeś/aś zaproszony/a do dołączenia do { -brand-name }, stworzonego przez
    { -org-name } systemu do tłumaczenia książek.

    Aby zarejestrować się przejdź pod poniższy adres URL

        { $url }

mail-invite-before-button =
    Zostałeś/aś zaproszony/a do dołączenia do { -brand-name }, stworzonego przez
    { -org-name } systemu do tłumaczenia książek.

    Aby zarejestrować się przejdź pod poniższy adres URL

mail-invite-register-button = Zarejestruj się

mail-invite-after-button =
    Albo skopiuj poniższy URL do paska przeglądarki:
    { -mail-url(url: $url, text: $url) }

mail-invite-footer = Powyższe zaproszenie dla { $email } do dołączenia do
    aplikacji { -brand-name } zostało wysłane przez członka
    zespołu { -org-name }.



## Password reset email

mail-reset-subject = Odzyskiwanie hasła

# Variables:
# - $username (string): user's name
# - $url (string): password reset URL
mail-reset-text =
    Cześć, { $username }.

    Aby zresetować hasło przejdź pod poniższy URL

        { $url }

    Jeżeli nie prosiłeś/aś o zresetowania hasła nie masz się czym martwić,
    twoje konto jest bezpieczne.

# Variables:
# - $username (string): user's name
mail-reset-before-button =
    Cześć, { $username }

    Aby zresetować swoje hasło kliknij poniższy guzik

mail-reset-button = Zresetuj hasło

# Variables:
# - $url (string): password reset URL
mail-reset-after-button =
    Albo skopiuj poniższy URL do paska przeglądarki
    { -mail-url(url: $url, text: $url) }

    Jeżeli nie prosiłeś/aś o zresetowania hasła nie masz się czym martwić,
    twoje konto jest bezpieczne.



## Notification email
#
# Notification emails are divided into section. Each section begins with
# mail-notify-group-header-KIND, where KIND is the type of events in this
# section. Each section then contains a list of events, formatted with
# mail-notify-event-KIND.

mail-notify-subject = Powiadomienie o postępie prac

mail-notify-footer =
    Pozdrawiamy, 
    Zespół { -org-name }

# Header displayed before notifications about module assignment.
mail-notify-group-header-assigned =
    Informacja o przydziale modułów do pracy:

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
    Moduł „{ $moduletitle }” ({ $moduleurl
    }) zostaje przekazany przez użytkownika { $actorname
    } do wykonania prac. { $bookcount ->
        [0] Moduł nie jest wykorzystywany w żadnej książce.
        [1] Moduł jest wykorzystywany w książce „{ $booktitle }” ({ $bookurl }).
       *[other] Moduł jest wykorzystywany w { $bookcount } książkach, w tym w „{
            $booktitle }” ({ $bookurl }).
    }
mail-notify-event-assigned =
    Moduł {
        -mail-url(url: $moduleurl, text: JOIN("„", $moduletitle, "”"))
    } zostaje przekazany przez użytkownika {
        -mail-url(url: $actorurl, text: $actorname)
    } do wykonania prac. { $bookcount ->
        [0] Moduł nie jest wykorzystywany w żadnej książce.
        [1] Moduł jest wykorzystywany w książce {
            -mail-url(url: $bookurl, text: $booktitle) }.
       *[other] Moduł jest wykorzystywany w { $bookcount } książkach, w tym w {
            -mail-url(url: $bookurl, text: $booktitle) }.
    }

# Header displayed before notifications about editing process finishing for
# drafts.
mail-notify-group-header-process-ended =
    Informacja o zakończeniu prac redakcyjnych:

# Notification about an editing process being finished for a draft.
#
# Variables:
# - $moduletitle (string): title of the module for whose draft the process ended
# - $moduleurl (string): URL to the module $moduletitle
mail-notify-event-process-ended-text =
    Z radością informujemy, że kończymy prace redakcyjne nad modułem „{
    $moduletitle }” ({ $moduleurl }).
mail-notify-event-process-ended =
    Z radością informujemy, że kończymy prace redakcyjne nad modułem {
    -mail-url(url: $moduleurl, text: $moduletitle) }.

# Notification about an editing process being cancelled for a draft.
#
# Variables:
# - $moduletitle (string): title of the module for whose draft the process was
#   cancelled
# - $moduleurl (string): URL to the module $moduletitle
mail-notify-event-process-cancelled-text =
    Proces redakcyjny dla modułu „{ $moduletitle }” ({ $moduleurl
    }) został zatrzymany.
mail-notify-event-process-cancelled =
    Proces redakcyjny dla modułu {
    -mail-url(url: $moduleurl, text: $moduletitle) } został zatrzymany.

# Header displayed before notifications about user being assigned to or removed
# from a slot in an editing process.
mail-notify-group-header-slot-assignment =
    Informacja o przydzieleniu zadań:

# Notification about user being assigned to a slot (or slots) in an editing
# process for a draft.
#
# Variables:
# - $drafttitle (string): title of the draft in which the user was assigned
# - $drafturl (string): URL to the draft $drafttitle
# - $slotname (string): name of the slot to which the user was assigned
mail-notify-event-slot-filled-text =
    Została przydzielona Ci rola { $slotname } modułu „{ $drafttitle }” ({
    $drafturl }).
mail-notify-event-slot-filled =
    Została przydzielona Ci rola { $slotname } modułu {
    -mail-url(url: $drafturl, text: $drafttitle) }.

# Notification about user being removed from a slot in an editing process for
# a draft.
#
# Variables:
# - $drafttitle (string): title of the draft in which the user was assigned
# - $drafturl (string): URL to the draft $drafttitle
# - $slotname (string): name of the slot to which the user was assigned
mail-notify-event-slot-vacated-text =
    Dotychczas przydzielona Ci rola { $slotname } modułu „{ $drafttitle }” ({
    $drafturl }) została przekazana innemu użytkownikowi.
mail-notify-event-slot-vacated =
    Dotychczas przydzielona Ci rola { $slotname
    } modułu {
        -mail-url(url: $drafturl, text: $drafttitle)
    } została przekazana innemu użytkownikowi.

# Header displayed before notifications about drafts moving between steps.
mail-notify-group-header-draft-advanced =
    Informacja o przepływie dokumentów w procesach redakcyjnych:

# Notification about a draft moving between steps.
#
# Variable:
# - $drafttitle (string): title of the draft in which the user was assigned
# - $drafturl (string): URL to the draft $drafttitle
# - $stepname (string): name of the step to which draft has moved
# - $bookcount (number): number of books in which the module is used
# - $booktitle (string): title of one of books in which the module is used
# - $bookurl (string): URL to the book $booktitle
mail-notify-event-draft-advanced-text =
    Moduł „{ $drafttitle }” ({ $drafturl
    }) zostaje przekazany z prośbą o wykonanie prac w zakresie: { $stepname
    }. { $bookcount ->
        [0] Moduł nie jest wykorzystywany w żadnej książce.
        [1] Moduł jest wykorzystywany w książce „{ $booktitle }” ({ $bookurl }).
       *[other] Moduł jest wykorzystywany w { $bookcount } książkach, w tym w „{
            $booktitle }” ({ $bookurl }).
    }
mail-notify-event-draft-advanced =
    Moduł { -mail-url(url: $drafturl, text: $drafttitle)
    } zostaje przekazany z prośbą o wykonanie prac w zakresie: { $stepname
    }. { $bookcount ->
        [0] Moduł nie jest wykorzystywany w żadnej książce.
        [1] Moduł jest wykorzystywany w książce {
            -mail-url(url: $bookurl, text: $booktitle) }.
       *[other] Moduł jest wykorzystywany w { $bookcount } książkach, w tym w {
            -mail-url(url: $bookurl, text: $booktitle) }.
    }

-mail-notify-unknown-text =
    Możesz zapoznać się z { $count ->
        [1] nim
       *[other] nimi
    } w centrum powiadomień ({ $url }).
-mail-notify-unknown =
    Możesz zapoznać się z { $count ->
        [1] nim
       *[other] nimi
    } w { -mail-url(url: $url, text: "centrum powiadomień") }.

# Message displayed at the end of the email if in there were unknown
# notifications in addition to normal notifications.
#
# Variables:
# - $count (number): Number of unknown notifications
# - $notification_centre_url (string): URL of the notifications centre
mail-notify-also-unknown-events-text =
    Oraz { $count ->
        [1] jedno inne zdarzenie którego
        [few] { $count} inne zdarzenia których
       *[many] { $count } innych zdarzeń których
    } nie jesteśmy w stanie przedstawić w wiadomości e-mail. {
        -mail-notify-unknown-text(count: $count, url: $notification_centre_url) }
mail-notify-also-unknown-events =
    Oraz { $count ->
        [1] jedno inne zdarzenie którego
        [few] { $count} inne zdarzenia których
       *[many] { $count } innych zdarzeń których
    } nie jesteśmy w stanie przedstawić w wiadomości e-mail.
    { -mail-notify-unknown(count: $count, url: $notification_centre_url) }

# Message displayed at the end of the email if in there were only unknown
# notifications.
#
# Variables:
# - $count (number): Number of unknown notifications
# - $notification_centre_url (string): URL of the notifications centre
mail-notify-only-unknown-events-text =
    Chcemy Cię poinformować o { $count ->
        [1] jednym zdarzeniu którego
       *[other] { $count } zdarzeniach których
    } nie jesteśmy w stanie przedstawić w wiadomości e-mail. {
        -mail-notify-unknown-text(count: $count, url: $notification_centre_url) }
mail-notify-only-unknown-events =
    Chcemy Cię poinformować o { $count ->
        [1] jednym zdarzeniu którego
       *[other] { $count } zdarzeniach których
    } nie jesteśmy w stanie przedstawić w wiadomości e-mail.
    { -mail-notify-unknown(count: $count, url: $notification_centre_url) }
