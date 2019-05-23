locale-name = Polski

## Login page

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

register-field-name = Imię

register-field-password = Hasło

register-field-repeat-password = Hasło

register-submit = Zarejestruj

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

mail-logo-alt = Logo OpenStax Polska™

mail-footer = Otrzymujesz tą wiadomość ponieważ jesteś członkiem Adaptarr!.

## Invitation email

mail-invite-subject = Zaproszenie

# Variables:
# - $url (string): registration URL
mail-invite-text =
    Zostałeś/aś zaproszony/a do dołączenia do Adaptarr!, stworzonego przez
    Katalyst Education systemu do tłumaczenia książek.

    Aby zarejestrować się przejdź pod poniższy adres URL

        { $url }

mail-invite-before-button =
    Zostałeś/aś zaproszony/a do dołączenia do Adaptarr!, stworzonego przez
    Katalyst Education systemu do tłumaczenia książek.

    Aby zarejestrować się przejdź pod poniższy adres URL

mail-invite-register-button = Zarejestruj się

mail-invite-after-button =
    Albo skopiuj poniższy URL do paska przeglądarki:
    <a href="{ $url }" target="_blank" rel="noopener">{ $url }</a>

mail-invite-footer = Otrzymujesz tą wiadomość, ponieważ ktoś zaprosił { $email }
    do dołączenia do Adaptarr!.

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
    <a href="{ $url }" target="_blank" rel="noopener">{ $url }</a>

    Jeżeli nie prosiłeś/aś o zresetowania hasła nie masz się czym martwić,
    twoje konto jest bezpieczne.
