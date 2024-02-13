use std::path::PathBuf;

use anyhow::anyhow;
use log::info;

/// handle --push-state/--pop-state
#[derive(Debug, Copy, Clone)]
struct OptStack {
    /// --as-needed
    pub as_needed: bool,
    /// -static
    pub link_static: bool,
}

#[derive(Debug, Clone)]
pub struct FileOpt {
    pub name: String,
    /// --as-needed
    pub as_needed: bool,
}

#[derive(Debug, Clone)]
pub struct LibraryOpt {
    pub name: String,
    /// --as-needed
    pub as_needed: bool,
    /// -static
    pub link_static: bool,
}

#[derive(Debug, Clone)]
pub enum ObjFileOpt {
    /// objfile
    File(FileOpt),
    /// -l namespec
    Library(LibraryOpt),
    /// --start-group
    StartGroup,
    /// --end-group
    EndGroup,
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
    pub obj_file: Vec<ObjFileOpt>,
}

/// parse arguments
pub fn parse_opts(args: &Vec<String>) -> anyhow::Result<Opt> {
    let mut opt = Opt::default();
    let mut cur_opt_stack = OptStack {
        as_needed: false,
        link_static: false,
    };
    let mut opt_stack = vec![];
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            // single dash
            s @ _ if s.starts_with("-L") => {
                // library search path argument
                opt.search_dir
                    .push(s.strip_prefix("-L").unwrap().to_string());
            }
            "-dynamic-linker" => {
                // dynamic linker argument
                opt.dynamic_linker = Some(
                    iter.next()
                        .ok_or(anyhow!("Missing dynamic linker after -dynamic-linker"))?
                        .to_string(),
                );
            }
            s @ _ if s.starts_with("-l") => {
                // library argument
                opt.obj_file.push(ObjFileOpt::Library(LibraryOpt {
                    name: s.strip_prefix("-l").unwrap().to_string(),
                    as_needed: cur_opt_stack.as_needed,
                    link_static: cur_opt_stack.link_static,
                }));
            }
            "-m" => {
                // emulation argument
                opt.emulation = Some(
                    iter.next()
                        .ok_or(anyhow!("Missing emulation after -m"))?
                        .to_string(),
                );
            }
            "-o" => {
                // output argument
                opt.output = Some(
                    iter.next()
                        .ok_or(anyhow!("Missing output after -o"))?
                        .to_string(),
                );
            }
            "-pie" => {
                opt.pie = true;
            }
            "-plugin" => {
                // skip plugin argument
                iter.next();
            }
            s @ _ if s.starts_with("-plugin-opt=") => {
                // ignored
            }
            "-static" => {
                cur_opt_stack.link_static = true;
            }

            // double dashes
            "--as-needed" => {
                cur_opt_stack.as_needed = true;
            }
            "--build-id" => {
                opt.build_id = true;
            }
            "--eh-frame-hdr" => {
                opt.eh_frame_hdr = true;
            }
            "--end-group" => {
                opt.obj_file.push(ObjFileOpt::EndGroup);
            }
            "--start-group" => {
                opt.obj_file.push(ObjFileOpt::StartGroup);
            }
            "--pop-state" => {
                cur_opt_stack = opt_stack.pop().unwrap();
            }
            "--push-state" => {
                opt_stack.push(cur_opt_stack);
            }
            // end of known flags
            s @ _ if s.starts_with("-") => {
                // unknown flag
                return Err(anyhow!("Unknown argument: {s}"));
            }
            s @ _ => {
                // object file argument
                opt.obj_file.push(ObjFileOpt::File(FileOpt {
                    name: s.to_string(),
                    as_needed: cur_opt_stack.as_needed,
                }));
            }
        }
    }
    Ok(opt)
}

fn lookup_file(name: &str, paths: &Vec<String>) -> anyhow::Result<PathBuf> {
    for path in paths {
        let mut p = PathBuf::from(path);
        p.push(name);
        if p.is_file() {
            info!("File {name} is found at {}", p.display());
            return Ok(p);
        }
    }
    Err(anyhow!("File {name} cannot be found"))
}

/// Do the actual linking
pub fn link(opt: &Opt) -> anyhow::Result<()> {
    info!("link with options: {opt:?}");
    let mut opt = opt.clone();

    // resolve library to actual files
    for obj_file in &mut opt.obj_file {
        // convert ObjFileOpt::Library to ObjFileOpt::File
        if let ObjFileOpt::Library(lib) = obj_file {
            if !lib.link_static {
                // lookup dynamic library first
                let path = format!("lib{}.so", lib.name);
                if let Ok(path) = lookup_file(&path, &opt.search_dir) {
                    *obj_file = ObjFileOpt::File(FileOpt {
                        name: format!("{}", path.display()),
                        as_needed: lib.as_needed,
                    });
                    continue;
                }
            }

            // lookup static library
            let path = format!("lib{}.a", lib.name);
            let path = lookup_file(&path, &opt.search_dir)?;
            *obj_file = ObjFileOpt::File(FileOpt {
                name: format!("{}", path.display()),
                as_needed: lib.as_needed,
            });
            continue;
        }
    }
    info!("options after path resolution: {opt:?}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_push_pop_state() {
        let opts = parse_opts(&vec![
            "-la".to_string(),
            "--push-state".to_string(),
            "--as-needed".to_string(),
            "-lb".to_string(),
            "--pop-state".to_string(),
            "-lc".to_string(),
        ])
        .unwrap();

        assert_eq!(opts.obj_file.len(), 3);
        if let ObjFileOpt::Library(lib) = &opts.obj_file[0] {
            assert_eq!(lib.name, "a");
            assert_eq!(lib.as_needed, false);
        } else {
            assert!(false);
        }

        if let ObjFileOpt::Library(lib) = &opts.obj_file[1] {
            assert_eq!(lib.name, "b");
            assert_eq!(lib.as_needed, true);
        } else {
            assert!(false);
        }

        if let ObjFileOpt::Library(lib) = &opts.obj_file[2] {
            assert_eq!(lib.name, "c");
            assert_eq!(lib.as_needed, false);
        } else {
            assert!(false);
        }
    }
}
