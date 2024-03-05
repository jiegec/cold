use anyhow::{anyhow, Context};
use log::{info, warn};
use object::{
    write::{elf::SectionIndex, StringId},
    Object, ObjectSection,
};
use std::{collections::BTreeMap, path::PathBuf};
use typed_arena::Arena;

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
pub enum ObjectFileOpt {
    /// ObjectFile
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
    /// ObjectFile
    pub obj_file: Vec<ObjectFileOpt>,
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
                opt.obj_file.push(ObjectFileOpt::Library(LibraryOpt {
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
                opt.obj_file.push(ObjectFileOpt::EndGroup);
            }
            "--start-group" => {
                opt.obj_file.push(ObjectFileOpt::StartGroup);
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
                opt.obj_file.push(ObjectFileOpt::File(FileOpt {
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

/// Resolve library namespec to paths
pub fn path_resolution(opt: &Opt) -> anyhow::Result<Opt> {
    // resolve library to actual files
    let mut opt = opt.clone();
    for obj_file in &mut opt.obj_file {
        // convert ObjectFileOpt::Library to ObjectFileOpt::File
        if let ObjectFileOpt::Library(lib) = obj_file {
            if !lib.link_static {
                // lookup dynamic library first
                let path = format!("lib{}.so", lib.name);
                if let Ok(path) = lookup_file(&path, &opt.search_dir) {
                    *obj_file = ObjectFileOpt::File(FileOpt {
                        name: format!("{}", path.display()),
                        as_needed: lib.as_needed,
                    });
                    continue;
                }
            }

            // lookup static library
            let path = format!("lib{}.a", lib.name);
            let path = lookup_file(&path, &opt.search_dir)?;
            *obj_file = ObjectFileOpt::File(FileOpt {
                name: format!("{}", path.display()),
                as_needed: lib.as_needed,
            });
            continue;
        }
    }
    Ok(opt)
}

#[derive(Debug, Clone)]
pub struct ObjectFile {
    pub name: String,
    /// --as-needed
    pub as_needed: bool,
    pub content: Vec<u8>,
}

#[derive(Default, Debug, Clone)]
pub struct OutputSection {
    pub name: String,
    pub content: Vec<u8>,
    pub offset: usize,
    // ELF
    pub section_index: Option<SectionIndex>,
    pub name_string_id: Option<StringId>,
}

/// Do the actual linking
pub fn link(opt: &Opt) -> anyhow::Result<()> {
    info!("Link with options: {opt:?}");

    let opt = path_resolution(&opt)?;
    info!("Options after path resolution: {opt:?}");

    // read files
    let mut files = vec![];
    for obj_file in &opt.obj_file {
        match obj_file {
            ObjectFileOpt::File(file_opt) => {
                info!("Reading {}", file_opt.name);
                files.push(ObjectFile {
                    name: file_opt.name.clone(),
                    as_needed: file_opt.as_needed,
                    content: std::fs::read(&file_opt.name)
                        .context(format!("Reading file {}", file_opt.name))?,
                });
            }
            ObjectFileOpt::Library(_) => unreachable!("Path resolution is not working"),
            ObjectFileOpt::StartGroup => warn!("--start-group unhandled"),
            ObjectFileOpt::EndGroup => warn!("--end-group unhandled"),
        }
    }

    // parse files and resolve symbols
    for file in &files {
        info!("Parsing {}", file.name);
        if file.name.ends_with(".a") {
            // archive
            let ar = object::read::archive::ArchiveFile::parse(file.content.as_slice())
                .context(format!("Parsing file {} as archive", file.name))?;
        } else {
            // object
            let obj = object::File::parse(file.content.as_slice())
                .context(format!("Parsing file {} as object", file.name))?;
            match obj {
                object::File::Elf64(elf) => {
                    // section name => section
                    let mut output_sections: BTreeMap<String, OutputSection> = BTreeMap::new();
                    for section in elf.sections() {
                        let name = section.name()?;
                        info!("Handling section {}", name);
                        let data = section.data()?;
                        if data.is_empty() {
                            continue;
                        }

                        if ![".symtab", ".strtab", ".shstrtab"].contains(&name)
                            && !name.starts_with(".rela")
                            && !name.is_empty()
                        {
                            // copy to output
                            let out = output_sections
                                .entry(name.to_string())
                                .or_insert_with(OutputSection::default);
                            out.name = name.to_string();
                            out.content.extend(data);
                        }
                    }

                    // create executable ELF
                    use object::write::elf::*;
                    let mut buffer = vec![];
                    let mut writer = Writer::new(object::Endianness::Little, true, &mut buffer);

                    // assign address to output sections
                    // and generate layout of executable
                    // assume executable is loaded at 0x400000
                    // the first page is reserved for ELF header & program header
                    writer.reserve_file_header();
                    // for simplicity, use one segment to map them all
                    writer.reserve_program_headers(1);

                    // thus sections begin at 0x401000
                    let load_address = 0x400000;
                    for (_name, output_section) in &mut output_sections {
                        output_section.offset = writer.reserve(output_section.content.len(), 4096);
                    }
                    info!("Output sections: {:?}", output_sections);

                    // reserve section headers and section header string table
                    writer.reserve_null_section_index();
                    // use typed-arena to avoid borrow to `output_sections`
                    let arena: Arena<u8> = Arena::new();
                    for (name, output_section) in &mut output_sections {
                        output_section.name_string_id =
                            Some(writer.add_section_name(arena.alloc_str(&name).as_bytes()));
                        output_section.section_index = Some(writer.reserve_section_index());
                    }
                    let shstrtab_section_index = writer.reserve_shstrtab_section_index();
                    writer.reserve_section_headers();
                    writer.reserve_shstrtab();

                    // all set! we can now write actual data to buffer
                    // ELF header
                    writer.write_file_header(&FileHeader {
                        os_abi: 0,
                        abi_version: 0,
                        e_type: object::elf::ET_EXEC,
                        e_machine: object::elf::EM_X86_64,
                        // assume that entrypoint is at the beginning of .text section for now
                        e_entry: (output_sections[".text"].offset + load_address) as u64,
                        e_flags: 0,
                    })?;
                    // program header
                    // ask kernel to load whole file into memory
                    writer.write_program_header(&ProgramHeader {
                        p_type: object::elf::PT_LOAD,
                        p_flags: object::elf::PF_X | object::elf::PF_W | object::elf::PF_R,
                        p_offset: 0,
                        p_vaddr: load_address as u64,
                        p_paddr: load_address as u64,
                        p_filesz: writer.reserved_len() as u64,
                        p_memsz: writer.reserved_len() as u64,
                        p_align: 4096,
                    });

                    // write section data
                    for (_name, output_section) in &mut output_sections {
                        writer.pad_until(output_section.offset);
                        writer.write(&output_section.content);
                    }

                    // write section headers
                    writer.write_null_section_header();
                    for (_name, output_section) in &mut output_sections {
                        writer.write_section_header(&SectionHeader {
                            name: output_section.name_string_id,
                            sh_type: object::elf::SHT_PROGBITS,
                            sh_flags: 0,
                            sh_addr: 0,
                            sh_offset: 0,
                            sh_size: 0,
                            sh_link: 0,
                            sh_info: 0,
                            sh_addralign: 0,
                            sh_entsize: 0,
                        });
                    }
                    writer.write_shstrtab_section_header();

                    // write section string table
                    writer.write_shstrtab();

                    // done, save to file
                    std::fs::write(opt.output.clone().unwrap(), buffer)?;
                }
                _ => return Err(anyhow!("Unsupported format of file {}", file.name)),
            }
        }
    }

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
        if let ObjectFileOpt::Library(lib) = &opts.obj_file[0] {
            assert_eq!(lib.name, "a");
            assert_eq!(lib.as_needed, false);
        } else {
            assert!(false);
        }

        if let ObjectFileOpt::Library(lib) = &opts.obj_file[1] {
            assert_eq!(lib.name, "b");
            assert_eq!(lib.as_needed, true);
        } else {
            assert!(false);
        }

        if let ObjectFileOpt::Library(lib) = &opts.obj_file[2] {
            assert_eq!(lib.name, "c");
            assert_eq!(lib.as_needed, false);
        } else {
            assert!(false);
        }
    }
}
