interaction-scan-entry-limit = Stopped after reading { $entries } folder entries: pick a folder further down to see the rest.

interaction-scan-time-limit = Stopped: it took too long. Found { $found } so far.

interaction-library-save-root = Save as root

interaction-library-save-root-tip = Keep this folder as a repository root: its repositories stay listed under All repositories on the start page and are found again by later scans.

interaction-settings-label-repository-roots = Repository roots

interaction-library-settings-hint = Folders whose repositories are listed under All repositories on the start page, for this profile.

interaction-library-title = ALL REPOSITORIES

interaction-library-manage = Repository roots…

interaction-library-empty = Add a folder you keep repositories in, and its repositories are listed here. Repositories next to the ones you open are listed too.

interaction-library-add-root = Add a root folder…

interaction-library-none-here = No repositories here yet.

interaction-library-all = All

interaction-library-recent = Recent

interaction-library-nearby = Nearby

interaction-library-stale = Stale

interaction-library-scanning = Scanning { $count ->
        [one] 1 folder
       *[other] { $count } folders
    }…

interaction-library-troubled = { $count ->
        [one] 1 folder couldn't be scanned completely
       *[other] { $count } folders couldn't be scanned completely
    }: see Repository roots…

interaction-library-hide = Hide from library

interaction-library-hide-tip = Hide from library: it isn't listed here again, whatever finds it. It stays in Recent, and can still be opened.

interaction-library-pick = Pick a folder you keep repositories in

interaction-library-roots-title = Repository roots

interaction-library-roots-about = gitcraft lists the repositories in these folders under All repositories on the start page, scanning them in the background now and then. It only looks at folders: it never runs git, fetches or connects anywhere for them.

interaction-library-no-roots = No roots yet.

interaction-library-enabled = Scan this folder

interaction-library-listed = { $count } listed

interaction-library-depth-0 = Only the folder

interaction-library-depth = { $depth ->
        [one] 1 level down
       *[other] { $depth } levels down
    }

interaction-library-move = Pick another folder for this root

interaction-library-remove = Remove this root (its repositories stay in Recent and in open tabs)

interaction-library-rescan = Rescan

interaction-library-rescan-all = Rescan all

interaction-library-nearby-switch = List repositories next to the ones I open

interaction-library-nearby-tip = When you open a repository, the other repositories in its folder are listed as Nearby (only that folder, one level deep; never your home folder, a drive or another machine). The last five folders are kept.

interaction-library-forget-nearby = Stop listing this folder's repositories

interaction-library-hidden = { $count ->
        [one] 1 repository hidden from the library
       *[other] { $count } repositories hidden from the library
    }

interaction-library-unhide = Show them again

interaction-library-slow = Waiting for a folder that is slow to answer: other scans start once it does.

interaction-library-off = Off

interaction-library-waiting = Waiting to scan

interaction-library-never = Not scanned yet

interaction-library-unavailable = Unavailable: { $reason } · { $when }

interaction-library-partial = Partial: { $why } · { $found } found · { $when }

interaction-library-done = { $found ->
        [one] 1 repository
       *[other] { $found } repositories
    } · { $when }

interaction-library-limit-repos = repository limit reached

interaction-library-limit-folders = folder limit reached

interaction-library-limit-time = took too long

interaction-library-limit-asked = stopped

interaction-library-limit-unreadable = some folders couldn't be read

interaction-library-limit-full = the library is full
