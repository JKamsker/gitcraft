//! In-process duplicates model the descriptor a concurrent fork holds before exec.
use super::*;
use std::cell::RefCell;

thread_local! {
    static RETAINED: RefCell<Option<Vec<std::fs::File>>> = const { RefCell::new(None) };
}

pub(in crate::resolver::park) struct Inherited;
impl Inherited {
    pub(in crate::resolver::park) fn new() -> Self {
        RETAINED.with(|slot| assert!(slot.borrow_mut().replace(Vec::new()).is_none()));
        Self
    }
    pub(in crate::resolver::park) fn count(&self) -> usize {
        RETAINED.with(|slot| slot.borrow().as_ref().unwrap().len())
    }
}
impl Drop for Inherited {
    fn drop(&mut self) {
        RETAINED.with(|slot| drop(slot.borrow_mut().take()));
    }
}

pub(super) fn retain(file: &std::fs::File) {
    RETAINED.with(|slot| {
        if let Some(files) = slot.borrow_mut().as_mut() {
            files.push(file.try_clone().unwrap());
        }
    });
}

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("gitcraft-lock-release-{}-{n}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        // A valid empty repository, built without spawning Git, also lets the
        // application repo_file implementation resolve its actual common dir.
        std::fs::create_dir_all(path.join(".git/objects")).unwrap();
        std::fs::create_dir(path.join(".git/refs")).unwrap();
        std::fs::write(path.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn lease_release_survives_duplicates_early_return_and_unwind() {
    let fixture = Fixture::new();
    let inherited = Inherited::new();
    let fail = || -> Result<(), String> {
        let _lease = acquire(&fixture.0, "early")?;
        Err("operation rejected".into())
    };
    assert_eq!(fail().unwrap_err(), "operation rejected");
    assert_eq!(inherited.count(), 1);
    let next = acquire(&fixture.0, "early").unwrap();
    assert!(acquire(&fixture.0, "early").is_err());
    drop(next);
    assert!(
        std::panic::catch_unwind(|| {
            let _lease = acquire(&fixture.0, "early").unwrap();
            panic!("operation panicked");
        })
        .is_err()
    );
    let next = acquire(&fixture.0, "early").unwrap();
    // Closing all old duplicates must not release the new independent holder.
    drop(inherited);
    assert!(acquire(&fixture.0, "early").is_err());
    drop(next);
    drop(acquire(&fixture.0, "early").unwrap());
}

#[test]
fn rejected_acquisition_preserves_an_independent_holder_and_inode() {
    let fixture = Fixture::new();
    let lease = acquire(&fixture.0, "holder").unwrap();
    let path = crate::agent::repo_file(&fixture.0, "parked-locks/holder").unwrap();
    use std::os::unix::fs::MetadataExt;
    let inode = std::fs::metadata(&path).unwrap().ino();
    let contender = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(acquire(&fixture.0, "holder").is_err());
    assert!(matches!(
        contender.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(lease);
    contender.try_lock().unwrap();
    assert!(acquire(&fixture.0, "holder").is_err());
    contender.unlock().unwrap();
    drop(acquire(&fixture.0, "holder").unwrap());
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
}
