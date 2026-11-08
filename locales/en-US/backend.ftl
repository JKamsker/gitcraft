backend-inbox-no-login = No { $forge } login is known for { $host }: add one with `tea login add`, or open a repository with a login so gitcraft can identify your pull requests.
backend-noun-merge-request = merge request
backend-noun-pull-request = pull request
backend-just-now-7ddb44 = just now
backend-relative-days = { $count ->
        [one] { $count } day ago
       *[other] { $count } days ago
    }
backend-relative-hours = { $count ->
        [one] { $count } hour ago
       *[other] { $count } hours ago
    }
backend-relative-minutes = { $count ->
        [one] { $count } minute ago
       *[other] { $count } minutes ago
    }
backend-relative-months = { $count ->
        [one] { $count } month ago
       *[other] { $count } months ago
    }
backend-relative-weeks = { $count ->
        [one] { $count } week ago
       *[other] { $count } weeks ago
    }
backend-relative-years = { $count ->
        [one] { $count } year ago
       *[other] { $count } years ago
    }
backend-search-date-formats = 2024-05-01, 2024-05, 2024, today, yesterday, or a time ago: 12h, 3d, 2w, 6m (months), 1y
backend-value-is-not-a-date-write-date-formats-at-most-max-years-years-ba-cd7a9d = “{ $value }” is not a date: write { $date_formats } (at most { $max_years } years back, days up to { $last_year })
