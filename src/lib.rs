#[derive(Debug, Clone)]
pub struct LibraryOpt {
    pub name: String,
    /// --as-needed
    pub as_needed: bool,
}

#[derive(Debug, Clone)]
pub enum ObjFileOpt {
    /// objfile
    File(String),
    /// -l namespec
    Library(LibraryOpt)
}

#[derive(Debug, Clone, Default)]
pub struct Opt {
    /// --build-id
    pub build_id: bool,
    /// --eh-frame-hdr
    pub eh_frame_hdr: bool,
    /// -pie
    pub pie: bool,
    /// -m emulation
    pub emulation: Option<String>,
    /// -o output
    pub output: Option<String>,
    /// -dynamic-linker
    pub dynamic_linker: Option<String>,
    /// -L searchdir
    pub search_dir: Vec<String>,
    /// objfile
    pub obj_file: Vec<ObjFileOpt>
}
