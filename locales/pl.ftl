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
    ["user:register:passwords-dont-match"] Hasła nie pasują
    ["user:register:email-changed"]
        Nie możesz zmienić adresu e-mail podczas rejestracji
    ["invitation:invalid"] Nieprawidłowy kod zaproszenia
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
    ["password:reset:invalid"] Niepoprawny kod resetowania hasła
    ["password:reset:passwords-dont-match"] Hasała nie pasują
   *[other] Wystąpił nieznany błąd { $code }
}
