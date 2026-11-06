//! The COM calls: each made inside a function that checks its own results, with interfaces
//! that live only for the call (the taskbar's is kept per thread).

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Foundation::HWND;
use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PSCoerceToCanonicalValue};
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW,
    ITaskbarList3, ShellLink, TBPF_ERROR, TBPF_INDETERMINATE, TBPF_NOPROGRESS, TBPF_NORMAL,
    TaskbarList,
};
use windows::core::{HSTRING, Interface};

use super::{Link, Progress, command_line};

fn error(what: &str, e: &windows::core::Error) -> String {
    format!("{what}: {e}")
}

/// COM for this thread, as a single-threaded apartment. The window's thread already is one
/// (winit sets OLE up there), so this only adds a reference, which keeps COM up for the
/// taskbar interface kept on it; the jump list is set from one thread that lives as long as
/// the app. Neither is ever uninitialised: both live until the process ends.
fn com() {
    // SAFETY: no reserved argument; a second call on the thread only adds a reference, and a
    // thread already in another apartment just says so (the call fails, nothing changes).
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
}

fn create<T: Interface>(class: &windows::core::GUID) -> windows::core::Result<T> {
    com();
    // SAFETY: an in-process class of the shell, by its id; the interface asked for is `T`'s.
    unsafe { CoCreateInstance(class, None, CLSCTX_INPROC_SERVER) }
}

thread_local! {
    static TASKBAR: RefCell<Option<ITaskbarList3>> = const { RefCell::new(None) };
}

pub(super) fn set_progress(hwnd: isize, progress: Progress) -> Result<(), String> {
    if hwnd == 0 {
        return Err("no window".to_owned());
    }
    TASKBAR.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            let taskbar: ITaskbarList3 = create(&TaskbarList).map_err(|e| error("taskbar", &e))?;
            // SAFETY: the object was just made; HrInit is the first call it needs.
            unsafe { taskbar.HrInit() }.map_err(|e| error("taskbar", &e))?;
            *cell = Some(taskbar);
        }
        let Some(taskbar) = cell.as_ref() else {
            return Err("taskbar".to_owned());
        };
        let window = HWND(hwnd as *mut c_void);
        // SAFETY: `window` is a window handle (an invalid one makes the calls fail, nothing
        // else); the values are in range.
        let result = unsafe {
            match progress {
                Progress::None => taskbar.SetProgressState(window, TBPF_NOPROGRESS),
                Progress::Indeterminate => taskbar.SetProgressState(window, TBPF_INDETERMINATE),
                Progress::Value(f) => {
                    taskbar
                        .SetProgressState(window, TBPF_NORMAL)
                        .and_then(|()| {
                            let done = (f.clamp(0.0, 1.0) * 1000.0) as u64;
                            taskbar.SetProgressValue(window, done, 1000)
                        })
                }
                Progress::Error => taskbar
                    .SetProgressState(window, TBPF_ERROR)
                    .and_then(|()| taskbar.SetProgressValue(window, 1000, 1000)),
            }
        };
        result.map_err(|e| error("taskbar progress", &e))
    })
}

/// A shortcut to `exe` with `link`'s arguments, title, tooltip and the program's icon.
fn shortcut(exe: &Path, link: &Link) -> windows::core::Result<IShellLinkW> {
    let shell_link: IShellLinkW = create(&ShellLink)?;
    let exe_s = HSTRING::from(exe.as_os_str());
    // SAFETY: each call gets a string that lives for the call; the property store is the
    // link's own, and the variant is a string made here.
    unsafe {
        shell_link.SetPath(&exe_s)?;
        shell_link.SetArguments(&HSTRING::from(command_line(&link.args)))?;
        shell_link.SetDescription(&HSTRING::from(link.tooltip.as_str()))?;
        shell_link.SetIconLocation(&exe_s, 0)?;
        let store: IPropertyStore = shell_link.cast()?;
        // A string as the title's own type (a wide string, as the shell's samples set it),
        // not the BSTR a PROPVARIANT is made from.
        let mut title = PROPVARIANT::from(link.title.as_str());
        PSCoerceToCanonicalValue(&PKEY_Title, &mut title)?;
        store.SetValue(&PKEY_Title, &title)?;
        store.Commit()?;
    }
    Ok(shell_link)
}

/// The arguments of the items the user removed from the jump list.
fn removed_args(removed: &IObjectArray) -> Vec<String> {
    let mut out = Vec::new();
    // SAFETY: the array is the one BeginList gave; indices are below its count; the buffer is
    // as long as said.
    unsafe {
        let count = removed.GetCount().unwrap_or(0);
        for i in 0..count {
            let Ok(link) = removed.GetAt::<IShellLinkW>(i) else {
                continue;
            };
            let mut buf = [0u16; 2048];
            if link.GetArguments(&mut buf).is_ok() {
                let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                out.push(String::from_utf16_lossy(&buf[..end]));
            }
        }
    }
    out
}

fn collection(exe: &Path, links: &[&Link]) -> windows::core::Result<IObjectArray> {
    let items: IObjectCollection = create(&EnumerableObjectCollection)?;
    for link in links {
        let made = shortcut(exe, link)?;
        // SAFETY: a shortcut made above, added to a collection made above.
        unsafe { items.AddObject(&made)? };
    }
    items.cast()
}

/// The arguments of the items the user removed from the jump list while gitcraft runs: Windows
/// says so once (at the next BeginList), and adding one again is refused.
static REMOVED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub(super) fn set_jump_list(
    exe: &Path,
    category: &str,
    recent: &[Link],
    tasks: &[Link],
) -> Result<(), String> {
    let run = || -> windows::core::Result<()> {
        let list: ICustomDestinationList = create(&DestinationList)?;
        let mut slots = 0u32;
        // SAFETY: `slots` lives for the call; the list is ours until CommitList or AbortList.
        let removed: IObjectArray = unsafe { list.BeginList(&mut slots)? };
        let gone = {
            let mut all = REMOVED.lock().map(|r| r.clone()).unwrap_or_default();
            all.extend(removed_args(&removed));
            if let Ok(mut r) = REMOVED.lock() {
                r.clone_from(&all);
            }
            all
        };
        let kept: Vec<&Link> = recent
            .iter()
            .filter(|l| !gone.contains(&command_line(&l.args)))
            .collect();
        let tasks: Vec<&Link> = tasks.iter().collect();
        let done = (|| {
            // The tasks first: the category can be refused on its own (the user turned off
            // "Show recently opened items", or an item they removed came back), which must
            // not cost Clone… and Open….
            if !tasks.is_empty() {
                let items = collection(exe, &tasks)?;
                // SAFETY: a collection made above.
                unsafe { list.AddUserTasks(&items)? };
            }
            if !kept.is_empty()
                && let Ok(items) = collection(exe, &kept)
            {
                // SAFETY: a title that lives for the call, a collection made above.
                let _ = unsafe { list.AppendCategory(&HSTRING::from(category), &items) };
            }
            // SAFETY: the list begun above.
            unsafe { list.CommitList() }
        })();
        if done.is_err() {
            // SAFETY: the list begun above, not committed.
            let _ = unsafe { list.AbortList() };
        }
        done
    };
    run().map_err(|e| error("jump list", &e))
}
