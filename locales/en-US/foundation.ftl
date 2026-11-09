# Shared formatting and language selection. IDs and argv are stable across locales.
format-number = { $value }
format-date = { $month }/{ $day }/{ $year }
settings-language-section = Language
settings-language-label = Interface language
settings-language-system = System language
settings-language-note = Applies immediately to translated interface text. Repository contents, commands, paths and ref names keep their original form.
shell-clone = Clone…
shell-clone-description = Clone a repository
shell-open = Open…
shell-open-description = Open a repository on this computer
foundation-repositories = { $count ->
    [one] Watching one repository
   *[other] Watching { $count } repositories
    }
settings-language-example = Example: { $number } · { $date }
