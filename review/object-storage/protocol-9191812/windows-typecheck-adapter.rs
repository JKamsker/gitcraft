#![allow(dead_code)]
mod remote { pub fn is_remote(_: &std::path::Path) -> bool { false } pub struct Output { pub code:i32,pub stdout:Vec<u8>,pub stderr:Vec<u8> } }
mod safe_read { pub fn final_path(_: &std::fs::File)->Option<std::path::PathBuf> {None} }
mod git {
    pub mod command {
        pub fn object_storage_output_with_env(_: &std::path::Path, _: &[&str], _:Vec<u8>, _:usize, _: &[(&'static str,String)], _: &dyn Fn()->bool)->Result<crate::remote::Output,String> {Err("compile-only adapter".into())}
    }
    mod object_storage { mod independent { mod local {
        #[path="/workspace/work/object-storage/src/git/object_storage/independent/held_windows.rs"] mod held;
    } } }
}
