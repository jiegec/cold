use crate::opt::{FileOpt, ObjectFileOpt, Opt};
use anyhow::{anyhow, bail, Context};
use object::elf::{
    Sym64, DT_JMPREL, DT_NEEDED, DT_PLTGOT, DT_PLTREL, DT_PLTRELSZ, DT_RELA, R_X86_64_JUMP_SLOT,
};
use object::write::elf::*;
use object::{
    elf::{DT_GNU_HASH, DT_HASH, DT_NULL, DT_SONAME, DT_STRSZ, DT_STRTAB, DT_SYMENT, DT_SYMTAB},
    write::{
        elf::{SectionIndex, Writer},
        StringId,
    },
    Object, ObjectSection, ObjectSymbol,
};
use object::{LittleEndian, ObjectKind};
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt, path::PathBuf};
use tracing::{info, info_span, warn};
use typed_arena::Arena;

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

// we want our own Relocation & RelocationTarget struct for easier handling
#[derive(Debug)]
pub enum RelocationTarget {
    // relocation against section with additional offset
    Section((String, u64)),
    // relocation against symbol
    Symbol(String),
}

#[derive(Debug)]
pub struct Relocation {
    // offset into the output section
    offset: u64,
    kind: object::RelocationKind,
    encoding: object::RelocationEncoding,
    size: u8,
    addend: i64,
    target: RelocationTarget,
}

#[derive(Debug)]
pub struct Symbol {
    // reside in which section
    section_name: String,
    // offset into the output section
    offset: u64,
    // indices in output .strtab
    symbol_name_string_id: Option<StringId>,
    // indices in output .dynstr
    symbol_name_dynamic_string_id: Option<StringId>,
    // local or global
    is_global: bool,
    // a plt symbol to dynamic library
    is_plt: bool,
}

#[derive(Debug, Clone)]
pub struct DynamicSymbol {
    name: String,
}

#[derive(Default, Debug)]
pub struct OutputSection {
    pub name: String,
    pub content: Vec<u8>,
    // offset from ELF load address
    pub offset: u64,
    // relocations in this section
    pub relocations: Vec<Relocation>,
    pub is_executable: bool,
    pub is_writable: bool,
    pub is_bss: bool,
    // indices in output ELF
    pub section_index: Option<SectionIndex>,
    pub name_string_id: Option<StringId>,
}

#[derive(Default, Debug)]
pub struct OutputRelocationSection {
    pub relocations: Vec<Rel>,
    // offset from ELF load address
    pub offset: u64,
    // indices in output ELF
    pub name_string_id: Option<StringId>,
}

#[derive(Default, Debug)]
pub struct Needed {
    pub name: String,
    // indices in output ELF
    pub name_string_id: Option<StringId>,
}

struct Linker<'a> {
    opt: Opt,
    files: Vec<ObjectFile>,

    // section name => section
    output_sections: BTreeMap<String, OutputSection>,

    // symbol table: symbol name => symbol
    symbols: BTreeMap<String, Symbol>,

    // dynamic symbols are saved in two parts:
    // plt dynamic symbols that are UNDEF
    plt_dynamic_symbols: Vec<DynamicSymbol>,
    // other defined dynamic symbols, sorted by hash bucket
    dynamic_symbols: Vec<DynamicSymbol>,

    // section address => offset
    section_address: BTreeMap<String, u64>,

    // elf writer
    writer: Writer<'a>,

    load_address: u64,

    // dynamic, dynsym, dynstr, hash, gnu_hash
    dynamic_section_index: SectionIndex,
    dynamic_section_offset: u64,
    dynsym_section_index: SectionIndex,
    dynsym_section_offset: u64,
    dynstr_section_offset: u64,
    hash_section_offset: u64,
    gnu_hash_section_offset: u64,
    dynamic_entries_count: usize,
    soname_dynamic_string_index: Option<StringId>,

    // dynamically link against shared libraries
    dynamic_link: bool,
    needed: Vec<Needed>,

    // output relocations
    output_relocations: BTreeMap<String, OutputRelocationSection>,
}

impl<'a> Linker<'a> {
    fn link(opt: &Opt) -> anyhow::Result<()> {
        info!("Link with options: {opt:?}");

        let opt = path_resolution(opt)?;
        info!("Options after path resolution: {opt:?}");

        let mut arena = Arena::new();
        let mut buffer = vec![];
        let mut linker = Linker {
            opt,
            files: vec![],
            output_sections: BTreeMap::new(),
            symbols: BTreeMap::new(),
            section_address: BTreeMap::new(),
            writer: Writer::new(object::Endianness::Little, true, &mut buffer),
            load_address: 0,
            dynamic_section_index: SectionIndex(0),
            dynamic_section_offset: 0,
            dynamic_entries_count: 0,
            dynsym_section_index: SectionIndex(0),
            dynsym_section_offset: 0,
            dynstr_section_offset: 0,
            hash_section_offset: 0,
            gnu_hash_section_offset: 0,
            soname_dynamic_string_index: None,
            dynamic_link: false,
            needed: vec![],
            output_relocations: BTreeMap::new(),
            dynamic_symbols: vec![],
            plt_dynamic_symbols: vec![],
        };
        linker.read_files()?;
        linker.parse_files()?;
        linker.reserve(&mut arena)?;
        linker.relocate()?;
        linker.write()?;

        // done, save to file
        let output = linker.opt.output.as_ref().unwrap();
        info!("Writing to executable {:?}", output);
        std::fs::write(output, buffer)?;

        // make executable
        let mut perms = std::fs::metadata(output)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(output, perms)?;

        Ok(())
    }

    fn read_files(&mut self) -> anyhow::Result<()> {
        let Linker { opt, files, .. } = self;

        // read files
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

        Ok(())
    }

    fn parse_files(&mut self) -> anyhow::Result<()> {
        let Linker {
            opt,
            files,
            output_sections,
            symbols,
            output_relocations,
            dynamic_symbols,
            plt_dynamic_symbols,
            ..
        } = self;

        // parse files and resolve symbols
        let mut objs = vec![];
        for file in files {
            info!("Parsing {}", file.name);
            if file.name.ends_with(".a") {
                // archive
                let ar = object::read::archive::ArchiveFile::parse(file.content.as_slice())
                    .context(format!("Parsing file {} as archive", file.name))?;
                for member in ar.members() {
                    let member = member?;
                    let name = format!("{}({})", file.name, std::str::from_utf8(member.name())?);
                    info!("Parsing {}", name);
                    let obj = object::File::parse(member.data(file.content.as_slice())?)
                        .context(format!("Parsing file {} as object", name))?;
                    objs.push((name, obj));
                }
            } else {
                // object
                let obj = object::File::parse(file.content.as_slice())
                    .context(format!("Parsing file {} as object", file.name))?;
                objs.push((file.name.clone(), obj));
            }
        }

        for (name, obj) in objs {
            let _span = info_span!("file", name).entered();
            match obj {
                object::File::Elf64(elf) => {
                    if elf.kind() == ObjectKind::Dynamic {
                        // linked against dynamic library
                        self.dynamic_link = true;
                        self.needed.push(Needed {
                            name: name.clone(),
                            name_string_id: None,
                        });

                        // walk through its dynamic symbols
                        // skip the first symbol which is null
                        for symbol in elf.dynamic_symbols().skip(1) {
                            if !symbol.is_undefined() {
                                let name = symbol.name()?;
                                info!("Defining dynamic symbol {}", name);
                                plt_dynamic_symbols.push(DynamicSymbol {
                                    name: name.to_string(),
                                });
                            }
                        }
                        continue;
                    }

                    // collect section sizes prior to this object
                    let section_sizes: BTreeMap<String, u64> = output_sections
                        .iter()
                        .map(|(key, value)| (key.clone(), value.content.len() as u64))
                        .collect();

                    for section in elf.sections() {
                        let name = section.name()?;
                        if !name.is_empty() {
                            let _span = info_span!("section", name).entered();
                            let data = section.data()?;
                            let (is_executable, is_writable) = match section.flags() {
                                object::SectionFlags::Elf { sh_flags } => {
                                    if ((sh_flags as u32) & object::elf::SHF_ALLOC) == 0 {
                                        // non-alloc, skip
                                        continue;
                                    } else {
                                        (
                                            ((sh_flags as u32) & object::elf::SHF_EXECINSTR) != 0,
                                            ((sh_flags as u32) & object::elf::SHF_WRITE) != 0,
                                        )
                                    }
                                }
                                _ => unimplemented!(),
                            };

                            // copy to output
                            let out = output_sections
                                .entry(name.to_string())
                                .or_insert_with(OutputSection::default);
                            out.name = name.to_string();
                            out.content.extend(data);
                            if (data.len() as u64) < section.size() {
                                // handle bss, extend with zero
                                out.content.resize(
                                    out.content.len() - data.len() + section.size() as usize,
                                    0,
                                );
                            }
                            out.is_executable |= is_executable;
                            out.is_writable |= is_writable;
                            out.is_bss |= section.kind() == object::SectionKind::UninitializedData;
                            for (offset, relocation) in section.relocations() {
                                match relocation.target() {
                                    object::RelocationTarget::Symbol(symbol_id) => {
                                        let symbol = elf.symbol_by_index(symbol_id)?;
                                        if symbol.kind() == object::SymbolKind::Section {
                                            // relocation to a section
                                            let section_index = symbol.section_index().unwrap();
                                            let target_section =
                                                elf.section_by_index(section_index)?;
                                            let target_section_name = target_section.name()?;
                                            info!(
                                                "Found relocation targeting section {}",
                                                target_section_name
                                            );

                                            out.relocations.push(Relocation {
                                                offset: offset
                                                    + *section_sizes.get(name).unwrap_or(&0),
                                                kind: relocation.kind(),
                                                encoding: relocation.encoding(),
                                                size: relocation.size(),
                                                addend: relocation.addend(),
                                                target: RelocationTarget::Section((
                                                    target_section_name.to_string(),
                                                    // record current size of section, because there can be existing content in the section from other object file
                                                    *section_sizes
                                                        .get(target_section_name)
                                                        .unwrap_or(&0),
                                                )),
                                            });
                                        } else {
                                            // relocation to a symbol
                                            let symbol_name = symbol.name()?;
                                            info!(
                                                "Found relocation targeting symbol {}",
                                                symbol_name
                                            );

                                            out.relocations.push(Relocation {
                                                offset: offset
                                                    + *section_sizes.get(name).unwrap_or(&0),
                                                kind: relocation.kind(),
                                                encoding: relocation.encoding(),
                                                size: relocation.size(),
                                                addend: relocation.addend(),
                                                target: RelocationTarget::Symbol(
                                                    symbol_name.to_string(),
                                                ),
                                            });
                                        }
                                    }
                                    _ => unimplemented!(),
                                };
                            }
                        }
                    }

                    // skip the first symbol which is null
                    for symbol in elf.symbols().skip(1) {
                        if !symbol.is_undefined()
                            && symbol.kind() != object::SymbolKind::Section
                            && symbol.kind() != object::SymbolKind::File
                        {
                            let name = symbol.name()?;
                            match symbol.section() {
                                object::SymbolSection::Section(section_index) => {
                                    let section = elf.section_by_index(section_index)?;
                                    let section_name = section.name()?;
                                    info!("Defining symbol {} from section {}", name, section_name);
                                    // offset: consider existing section content from other files
                                    let offset = symbol.address()
                                        + section_sizes.get(section_name).unwrap_or(&0);
                                    symbols.insert(
                                        name.to_string(),
                                        Symbol {
                                            section_name: section_name.to_string(),
                                            offset,
                                            symbol_name_string_id: None,
                                            symbol_name_dynamic_string_id: None,
                                            is_global: symbol.is_global(),
                                            is_plt: false,
                                        },
                                    );

                                    if symbol.is_global() && opt.shared {
                                        // export GLOBAL symbols in dynsym
                                        dynamic_symbols.push(DynamicSymbol {
                                            name: name.to_string(),
                                        });
                                    }
                                }
                                _ => bail!(
                                    "Symbol kind is {:?}, symbol section is {:?}",
                                    symbol.kind(),
                                    symbol.section(),
                                ),
                            }
                        }
                    }
                }
                _ => return Err(anyhow!("Unsupported format of file {}", name)),
            }
        }

        if opt.shared || self.dynamic_link {
            // add _DYNAMIC symbol
            symbols.insert(
                "_DYNAMIC".to_string(),
                Symbol {
                    section_name: ".dynamic".to_string(),
                    offset: 0,
                    symbol_name_string_id: None,
                    symbol_name_dynamic_string_id: None,
                    is_global: false,
                    is_plt: false,
                },
            );
        }

        // sort dynamic symbols by gnu hash bucket
        let bucket_count = dynamic_symbols.len();
        dynamic_symbols.sort_by_key(|sym| {
            let hash = object::elf::gnu_hash(sym.name.as_bytes());
            hash % bucket_count as u32
        });

        // handle dynamic symbols: construct .plt, .got.plt
        if self.dynamic_link {
            assert!(!output_sections.contains_key(".plt"));
            let mut plt = OutputSection {
                name: ".plt".to_string(),
                is_executable: true,
                ..OutputSection::default()
            };

            // first entry in plt:
            plt.content.extend(vec![
                // ff 35 xx xx xx xx push .got.plt+8(%rip)
                0xff, 0x35, 0x00, 0x00, 0x00, 0x00,
                // ff 25 xx xx xx xx jmp *.got.plt+16(%rip)
                0xff, 0x25, 0x00, 0x00, 0x00, 0x00, // 0f 1f 40 00       nop
                0x0f, 0x1f, 0x40, 0x00,
            ]);
            // relocation for push .got.plt+8(rip)
            plt.relocations.push(Relocation {
                offset: 0x2,
                kind: object::RelocationKind::Relative,
                encoding: object::RelocationEncoding::Generic,
                size: 32,
                addend: 8 - 4,
                target: RelocationTarget::Section((".got.plt".to_string(), 0)),
            });
            // relocation for jmp *.got.plt+16(%rip)
            plt.relocations.push(Relocation {
                offset: 0x8,
                kind: object::RelocationKind::Relative,
                encoding: object::RelocationEncoding::Generic,
                size: 32,
                addend: 16 - 4,
                target: RelocationTarget::Section((".got.plt".to_string(), 0)),
            });
            output_sections.insert(".plt".to_string(), plt);

            // got contents:
            assert!(!output_sections.contains_key(".got.plt"));
            let mut got_plt = OutputSection {
                name: ".got.plt".to_string(),
                ..OutputSection::default()
            };
            got_plt.content.extend(vec![
                // 0: address of .dynamic section
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                // 1: 0, reserved for ld.so
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                // 2: 0, reserved for ld.so
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ]);
            // address of .dynamic section
            got_plt.relocations.push(Relocation {
                offset: 0x0,
                kind: object::RelocationKind::Absolute,
                encoding: object::RelocationEncoding::Generic,
                size: 64,
                addend: 0,
                target: RelocationTarget::Section((".dynamic".to_string(), 0)),
            });
            output_sections.insert(".got.plt".to_string(), got_plt);

            // add _GLOBAL_OFFSET_TABLE_ symbol
            symbols.insert(
                "_GLOBAL_OFFSET_TABLE_".to_string(),
                Symbol {
                    section_name: ".got.plt".to_string(),
                    offset: 0,
                    symbol_name_string_id: None,
                    symbol_name_dynamic_string_id: None,
                    is_global: false,
                    is_plt: false,
                },
            );

            for (idx, dyn_sym) in plt_dynamic_symbols.iter().enumerate() {
                // redirect the symbol to plt
                let plt = output_sections.get_mut(".plt").unwrap();
                let plt_offset = plt.content.len() as u64;

                // each entry in plt:
                // ff 25 xx xx xx xx jmp *.got.plt+yy(%rip)
                plt.content.extend(vec![0xff, 0x25, 0x00, 0x00, 0x00, 0x00]);
                // 68 xx xx xx xx    push index
                plt.content.push(0x68);
                plt.content.extend_from_slice(&(idx as u32).to_le_bytes());
                // e9 xx xx xx xx    jmp plt_first_entry
                plt.content.extend(vec![0xe9, 0x00, 0x00, 0x00, 0x00]);

                // relocation for jmp *.got.plt+yy(%rip)
                plt.relocations.push(Relocation {
                    offset: 0x2 + plt_offset,
                    kind: object::RelocationKind::Relative,
                    encoding: object::RelocationEncoding::Generic,
                    size: 32,
                    // each got entry: 8 bytes
                    // 24: got header
                    addend: (idx as i64 * 8 + 24) - 4,
                    target: RelocationTarget::Section((".got.plt".to_string(), 0)),
                });
                // relocation for jmp plt_first_entry
                plt.relocations.push(Relocation {
                    offset: 12 + plt_offset,
                    kind: object::RelocationKind::Relative,
                    encoding: object::RelocationEncoding::Generic,
                    size: 32,
                    addend: 0 - 4,
                    target: RelocationTarget::Section((".plt".to_string(), 0)),
                });

                // add entry in .got.plt
                let got_plt = output_sections.get_mut(".got.plt").unwrap();
                let got_offset = got_plt.content.len() as u64;
                // 8 bytes for absolute address
                got_plt.content.extend(vec![0; 8]);

                // static relocation to the next instruction in plt in binary
                got_plt.relocations.push(Relocation {
                    offset: got_offset,
                    kind: object::RelocationKind::Absolute,
                    encoding: object::RelocationEncoding::Generic,
                    size: 64,
                    addend: plt_offset as i64 + 6, // point to push index
                    target: RelocationTarget::Section((".plt".to_string(), 0)),
                });

                // add dynamic relocation R_X86_64_JUMP_SLOT to actual symbol
                output_relocations
                    .entry(".rela.plt".to_string())
                    .or_default()
                    .relocations
                    .push(Rel {
                        r_offset: got_offset,
                        r_sym: (idx + 1) as u32,
                        r_type: R_X86_64_JUMP_SLOT,
                        r_addend: 0,
                    });

                symbols.insert(
                    dyn_sym.name.clone(),
                    Symbol {
                        section_name: ".plt".to_string(),
                        offset: plt_offset,
                        symbol_name_string_id: None,
                        symbol_name_dynamic_string_id: None,
                        is_global: true,
                        is_plt: true,
                    },
                );
            }
        }

        if !opt.shared && self.dynamic_link {
            let mut interp = OutputSection {
                name: ".interp".to_string(),
                ..OutputSection::default()
            };
            interp
                .content
                .extend_from_slice(opt.dynamic_linker.as_ref().unwrap().as_bytes());
            // NULL terminated string
            interp.content.push(0);
            output_sections.insert(".interp".to_string(), interp);
        }

        Ok(())
    }

    fn reserve(&mut self, arena: &'a mut Arena<u8>) -> anyhow::Result<()> {
        let Linker {
            opt,
            output_sections,
            symbols,
            dynamic_symbols,
            plt_dynamic_symbols,
            writer,
            output_relocations,
            dynamic_section_index,
            dynsym_section_index,
            ..
        } = self;

        // assign address to output sections
        // and generate layout of executable
        // assume executable is loaded at 0x400000
        self.load_address = if opt.shared { 0 } else { 0x400000 };
        // the first page is reserved for ELF header & program header
        writer.reserve_file_header();
        // for simplicity, use one segment to map them all
        let mut program_headers_count = 1; // PT_LOAD
        if opt.shared || self.dynamic_link {
            // PT_DYNAMIC
            program_headers_count += 1;
        }
        if !opt.shared && self.dynamic_link {
            // PT_INTERP
            program_headers_count += 1;
        }
        writer.reserve_program_headers(program_headers_count);

        // thus sections begin at 0x401000
        for (_name, output_section) in output_sections.iter_mut() {
            output_section.offset = writer.reserve(output_section.content.len(), 4096) as u64;
        }
        info!("Got {} output sections", output_sections.len());

        // reserve .rela.xx sections
        for (_name, output_section) in output_relocations.iter_mut() {
            output_section.offset = writer.reserve(
                output_section.relocations.len()
                    * std::mem::size_of::<object::elf::Rela64<LittleEndian>>(),
                8,
            ) as u64;
        }

        // reserve section headers
        writer.reserve_null_section_index();
        // use typed-arena to avoid borrow to `output_sections`
        for (name, output_section) in output_sections.iter_mut() {
            output_section.name_string_id =
                Some(writer.add_section_name(arena.alloc_str(name).as_bytes()));
            output_section.section_index = Some(writer.reserve_section_index());
        }
        for (name, output_section) in output_relocations.iter_mut() {
            output_section.name_string_id =
                Some(writer.add_section_name(arena.alloc_str(name).as_bytes()));
            writer.reserve_section_index();
        }
        let _symtab_section_index = writer.reserve_symtab_section_index();
        let _strtab_section_index = writer.reserve_strtab_section_index();
        let _shstrtab_section_index = writer.reserve_shstrtab_section_index();
        if opt.shared || self.dynamic_link {
            // .dynamic, .dynsym, .dynstr, .hash, .gnu_hash
            *dynamic_section_index = writer.reserve_dynamic_section_index();
            *dynsym_section_index = writer.reserve_dynsym_section_index();
            let _dynstr_section_index = writer.reserve_dynstr_section_index();
            if opt.hash_style.sysv {
                let _hash_section_index = writer.reserve_hash_section_index();
            }
            if opt.hash_style.gnu {
                let _gnu_hash_section_index = writer.reserve_gnu_hash_section_index();
            }
        }
        writer.reserve_section_headers();

        // prepare symbol table
        writer.reserve_null_symbol_index();
        for (symbol_name, symbol) in symbols.iter_mut() {
            symbol.symbol_name_string_id =
                Some(writer.add_string(arena.alloc_str(symbol_name).as_bytes()));
            writer.reserve_symbol_index(None);
        }

        // reserve symtab, strtab and shstrtab
        writer.reserve_symtab();
        writer.reserve_strtab();
        writer.reserve_shstrtab();

        // reserve dynamic, dynsym, dynstr, hash and gnu_hash
        self.dynamic_entries_count = 5;
        if opt.shared || self.dynamic_link {
            // dynamic entries:
            // 1. HASH -> .hash
            // 2. GNU_HASH -> .gnu_hash
            // 3. STRTAB -> .dynstr
            // 4. SYMTAB -> .dynsym
            // 5. STRSZ
            // 6. SYMENT
            // 7. SONAME
            // 8. PLTGOT -> .got.plt
            // 9. PLTRELSZ
            // 10. PLTREL
            // 11. JMPREL -> .rela.plt
            // 12. NEEDED
            // 13. NULL
            if opt.hash_style.sysv {
                self.dynamic_entries_count += 1;
            }
            if opt.hash_style.gnu {
                self.dynamic_entries_count += 1;
            }
            if opt.soname.is_some() {
                self.dynamic_entries_count += 1;
            }
            if self.dynamic_link {
                // PLTGOT, PLTRELSZ, PLTREL, JMPREL
                self.dynamic_entries_count += 4;
            }
            self.dynamic_entries_count += self.needed.len();

            // align to 8 bytes boundary
            self.dynamic_section_offset = writer.reserve_dynamic(self.dynamic_entries_count) as u64;

            // dynamic symbols
            writer.reserve_null_dynamic_symbol_index();
            for dyn_sym in plt_dynamic_symbols.iter().chain(dynamic_symbols.iter()) {
                let symbol = symbols.get_mut(&dyn_sym.name).unwrap();
                symbol.symbol_name_dynamic_string_id =
                    Some(writer.add_dynamic_string(arena.alloc_str(&dyn_sym.name).as_bytes()));
                writer.reserve_dynamic_symbol_index();
            }

            if let Some(soname) = &opt.soname {
                self.soname_dynamic_string_index =
                    Some(writer.add_dynamic_string(arena.alloc_str(soname).as_bytes()))
            };

            for needed in &mut self.needed {
                needed.name_string_id =
                    Some(writer.add_dynamic_string(arena.alloc_str(&needed.name).as_bytes()));
            }

            self.dynsym_section_offset = writer.reserve_dynsym() as u64;

            // dynamic string
            self.dynstr_section_offset = writer.reserve_dynstr() as u64;

            // hash table
            let plt_dynamic_symbols_count = plt_dynamic_symbols.len() as u32;
            let dynamic_symbols_count = dynamic_symbols.len() as u32;
            if opt.hash_style.sysv {
                // chain count: 1 extra element for NULL symbol
                self.hash_section_offset = writer.reserve_hash(
                    plt_dynamic_symbols_count + dynamic_symbols_count,
                    plt_dynamic_symbols_count + dynamic_symbols_count + 1,
                ) as u64;
            }

            // gnu hash table
            if opt.hash_style.gnu {
                // plt dynamic symbols are not included in gnu hash table
                self.gnu_hash_section_offset =
                    writer.reserve_gnu_hash(1, dynamic_symbols_count, dynamic_symbols_count) as u64;
            }
        };

        Ok(())
    }

    fn write(&mut self) -> anyhow::Result<()> {
        let Linker {
            opt,
            output_sections,
            output_relocations,
            symbols,
            dynamic_symbols,
            plt_dynamic_symbols,
            writer,
            soname_dynamic_string_index,
            section_address,
            ..
        } = self;

        // all set! we can now write actual data to buffer
        // compute entrypoint address
        let entry_address = if opt.shared {
            // building shared library, no entrypoint
            0
        } else {
            let entry_symbol = &symbols["_start"];
            section_address[&entry_symbol.section_name] + entry_symbol.offset
        };

        // ELF header
        writer.write_file_header(&FileHeader {
            os_abi: 0,
            abi_version: 0,
            e_type: if opt.shared {
                object::elf::ET_DYN
            } else {
                object::elf::ET_EXEC
            },
            e_machine: object::elf::EM_X86_64,
            // assume that entrypoint is pointed at _start
            e_entry: entry_address,
            e_flags: 0,
        })?;
        // program header
        // ask kernel to load segments into memory
        writer.write_program_header(&ProgramHeader {
            p_type: object::elf::PT_LOAD,
            p_flags: object::elf::PF_X | object::elf::PF_W | object::elf::PF_R,
            p_offset: 0,
            p_vaddr: self.load_address,
            p_paddr: self.load_address,
            p_filesz: writer.reserved_len() as u64,
            p_memsz: writer.reserved_len() as u64,
            p_align: 4096,
        });
        if opt.shared || self.dynamic_link {
            writer.write_program_header(&ProgramHeader {
                p_type: object::elf::PT_DYNAMIC,
                p_flags: object::elf::PF_W | object::elf::PF_R,
                p_offset: self.dynamic_section_offset,
                p_vaddr: self.dynamic_section_offset + self.load_address,
                p_paddr: self.dynamic_section_offset + self.load_address,
                p_filesz: (self.dynamic_entries_count
                    * std::mem::size_of::<object::elf::Dyn64<object::LittleEndian>>())
                    as u64,
                p_memsz: (self.dynamic_entries_count
                    * std::mem::size_of::<object::elf::Dyn64<object::LittleEndian>>())
                    as u64,
                p_align: 8,
            });
        }
        if !opt.shared && self.dynamic_link {
            writer.write_program_header(&ProgramHeader {
                p_type: object::elf::PT_INTERP,
                p_flags: object::elf::PF_R,
                p_offset: output_sections[".interp"].offset,
                p_vaddr: section_address[".interp"],
                p_paddr: section_address[".interp"],
                p_filesz: output_sections[".interp"].content.len() as u64,
                p_memsz: output_sections[".interp"].content.len() as u64,
                p_align: 1,
            });
        }

        // write section data
        for (_name, output_section) in output_sections.iter() {
            writer.pad_until(output_section.offset as usize);
            writer.write(&output_section.content);
        }
        for (_name, output_section) in output_relocations.iter() {
            writer.pad_until(output_section.offset as usize);
            for rel in &output_section.relocations {
                // turn offset into absolute
                let mut rel = rel.clone();
                rel.r_offset += section_address[".got.plt"];
                writer.write_relocation(true, &rel);
            }
        }

        // write section headers
        writer.write_null_section_header();
        for (name, output_section) in output_sections.iter() {
            let mut flags = object::elf::SHF_ALLOC;
            if output_section.is_executable {
                flags |= object::elf::SHF_EXECINSTR;
            }
            if output_section.is_writable {
                flags |= object::elf::SHF_WRITE;
            }

            writer.write_section_header(&SectionHeader {
                name: output_section.name_string_id,
                sh_type: if output_section.is_bss {
                    object::elf::SHT_NOBITS
                } else {
                    object::elf::SHT_PROGBITS
                },
                sh_flags: flags as u64,
                sh_addr: section_address[name],
                sh_offset: output_section.offset,
                sh_size: output_section.content.len() as u64,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
            });
        }
        for (name, output_section) in output_relocations.iter() {
            let flags = object::elf::SHF_ALLOC | object::elf::SHF_INFO_LINK;

            let entsize = std::mem::size_of::<object::elf::Rela64<LittleEndian>>();
            writer.write_section_header(&SectionHeader {
                name: output_section.name_string_id,
                sh_type: object::elf::SHT_RELA,
                sh_flags: flags as u64,
                sh_addr: section_address[name],
                sh_offset: output_section.offset,
                sh_size: (output_section.relocations.len() * entsize) as u64,
                sh_link: self.dynsym_section_index.0, // associated to .dynsym
                sh_info: output_sections
                    .get(".got.plt")
                    .unwrap()
                    .section_index
                    .unwrap()
                    .0,
                sh_addralign: 8,
                sh_entsize: entsize as u64,
            });
        }
        writer.write_symtab_section_header(
            1 + symbols.iter().filter(|(_name, sym)| !sym.is_global).count() as u32,
        ); // +1: one extra null symbol at the beginning
        writer.write_strtab_section_header();
        writer.write_shstrtab_section_header();
        if opt.shared || self.dynamic_link {
            writer.write_dynamic_section_header(self.dynamic_section_offset + self.load_address);
            writer.write_dynsym_section_header(self.dynsym_section_offset + self.load_address, 1); // one local: null symbol
            writer.write_dynstr_section_header(self.dynstr_section_offset + self.load_address);
            if opt.hash_style.sysv {
                writer.write_hash_section_header(self.hash_section_offset + self.load_address);
            }
            if opt.hash_style.gnu {
                writer.write_gnu_hash_section_header(
                    self.gnu_hash_section_offset + self.load_address,
                );
            }
        }

        // write symbol table
        writer.write_null_symbol();
        let mut symbols_vec: Vec<_> = symbols.iter().collect();
        // local symbols first
        symbols_vec.sort_by_key(|(_name, sym)| sym.is_global);
        for (_symbol_name, symbol) in symbols_vec {
            let address = section_address[&symbol.section_name] + symbol.offset;
            writer.write_symbol(&Sym {
                name: symbol.symbol_name_string_id,
                section: if symbol.is_plt {
                    None // UNDEF
                } else if symbol.section_name == ".dynamic" {
                    Some(self.dynamic_section_index)
                } else {
                    output_sections[&symbol.section_name].section_index
                },
                st_info: if symbol.is_global {
                    (object::elf::STB_GLOBAL) << 4
                } else {
                    (object::elf::STB_LOCAL) << 4
                },
                st_other: 0,
                st_shndx: 0,
                st_value: if symbol.is_plt { 0 } else { address },
                st_size: 0,
            });
        }

        // write string table
        writer.write_strtab();

        // write section string table
        writer.write_shstrtab();

        // shared library or dynamic linking
        if opt.shared || self.dynamic_link {
            // dynamic entries:
            // 1. HASH -> .hash
            // 2. GNU_HASH -> .gnu_hash
            // 3. STRTAB -> .dynstr
            // 4. SYMTAB -> .dynsym
            // 5. STRSZ
            // 6. SYMENT
            // 7. SONAME
            // 8. PLTGOT -> .got.plt
            // 9. PLTRELSZ
            // 10. PLTREL
            // 11. JMPREL -> .rela.plt
            // 12. NEEDED
            // 13. NULL
            writer.write_align_dynamic();
            if opt.hash_style.sysv {
                writer.write_dynamic(DT_HASH, self.hash_section_offset + self.load_address);
            }
            if opt.hash_style.gnu {
                writer.write_dynamic(
                    DT_GNU_HASH,
                    self.gnu_hash_section_offset + self.load_address,
                );
            }
            writer.write_dynamic(DT_STRTAB, self.dynstr_section_offset + self.load_address);
            writer.write_dynamic(DT_SYMTAB, self.dynsym_section_offset + self.load_address);
            let strsz = writer.dynstr_len() as u64;
            writer.write_dynamic(DT_STRSZ, strsz); // size of dynamic string table
            writer.write_dynamic(DT_SYMENT, std::mem::size_of::<Sym64<LittleEndian>>() as u64); // entry size
            if let Some(soname_dynamic_string_index) = &soname_dynamic_string_index {
                writer.write_dynamic_string(DT_SONAME, *soname_dynamic_string_index);
            }
            if self.dynamic_link {
                writer.write_dynamic(DT_PLTGOT, section_address[".got.plt"]);
                writer.write_dynamic(
                    DT_PLTRELSZ,
                    (output_relocations[".rela.plt"].relocations.len()
                        * std::mem::size_of::<object::elf::Rela64<LittleEndian>>())
                        as u64,
                );
                writer.write_dynamic(DT_PLTREL, DT_RELA as u64);
                writer.write_dynamic(DT_JMPREL, section_address[".rela.plt"]);
            }
            for needed in &self.needed {
                writer.write_dynamic_string(DT_NEEDED, needed.name_string_id.unwrap());
            }

            writer.write_dynamic(DT_NULL, 0);

            // write dynamic symbols
            writer.write_null_dynamic_symbol();
            for dyn_sym in plt_dynamic_symbols.iter().chain(dynamic_symbols.iter()) {
                let symbol = symbols.get(&dyn_sym.name).unwrap();
                let address = section_address[&symbol.section_name] + symbol.offset;
                writer.write_dynamic_symbol(&Sym {
                    name: symbol.symbol_name_dynamic_string_id,
                    section: if symbol.is_plt {
                        None
                    } else {
                        output_sections[&symbol.section_name].section_index
                    },
                    st_info: (object::elf::STB_GLOBAL) << 4,
                    st_other: 0,
                    st_shndx: 0,
                    st_value: if symbol.is_plt { 0 } else { address },
                    st_size: 0,
                });
            }

            // write dynamic string table
            writer.write_dynstr();

            // write hash table
            if opt.hash_style.sysv {
                writer.write_hash(
                    (plt_dynamic_symbols.len() + dynamic_symbols.len()) as u32,
                    (plt_dynamic_symbols.len() + dynamic_symbols.len()) as u32 + 1, // + 1 for NULL symbol at start
                    |idx| {
                        // compute sysv hash of symbol name
                        // 0 is reserved for null, skip
                        if idx == 0 {
                            None
                        } else if idx <= plt_dynamic_symbols.len() as u32 {
                            // UNDEF
                            None
                        } else {
                            Some(object::elf::hash(
                                dynamic_symbols[idx as usize - 1 - plt_dynamic_symbols.len()]
                                    .name
                                    .as_bytes(),
                            ))
                        }
                    },
                );
            }

            // write gnu hash table
            if opt.hash_style.gnu {
                writer.write_gnu_hash(
                    1 + plt_dynamic_symbols.len() as u32, // skip NULL symbol and plt UNDEF symbols
                    1,
                    1,
                    dynamic_symbols.len() as u32,
                    dynamic_symbols.len() as u32,
                    |idx| {
                        // compute gnu hash of symbol name
                        object::elf::gnu_hash(dynamic_symbols[idx as usize].name.as_bytes())
                    },
                );
            }
        }

        Ok(())
    }

    fn relocate(&mut self) -> anyhow::Result<()> {
        let Linker {
            opt,
            output_sections,
            output_relocations,
            symbols,
            section_address,
            ..
        } = self;

        // compute mapping from section name to virtual address
        for (name, output_section) in output_sections.iter() {
            section_address.insert(name.clone(), output_section.offset + self.load_address);
        }
        for (name, output_section) in output_relocations.iter() {
            section_address.insert(name.clone(), output_section.offset + self.load_address);
        }
        if opt.shared || self.dynamic_link {
            section_address.insert(
                ".dynamic".to_string(),
                self.load_address + self.dynamic_section_offset,
            );
        }

        // compute relocation
        for (name, output_section) in output_sections.iter_mut() {
            let _span = info_span!("section", name = name).entered();
            for (index, relocation) in output_section.relocations.iter().enumerate() {
                let _span = info_span!("relocation", index = index).entered();
                let target_address = match &relocation.target {
                    RelocationTarget::Section((name, offset)) => {
                        info!("Relocation is targeting section {}", name);
                        section_address[name] + offset
                    }
                    RelocationTarget::Symbol(name) => {
                        info!("Relocation is targeting symbol {}", name);
                        let symbol = &symbols[name];
                        section_address[&symbol.section_name] + symbol.offset
                    }
                };

                // symbol
                let s = target_address as i64;
                // addend
                let a = relocation.addend;
                // pc
                let p = self.load_address + output_section.offset + relocation.offset;

                match (relocation.kind, relocation.encoding, relocation.size) {
                    // R_X86_64_64
                    (object::RelocationKind::Absolute, object::RelocationEncoding::Generic, 64) => {
                        info!("Relocation type is R_X86_64_64");
                        // S + A
                        let value = s.wrapping_add(a);
                        output_section.content
                            [(relocation.offset) as usize..(relocation.offset + 8) as usize]
                            .copy_from_slice(&(value as i64).to_le_bytes());
                    }
                    // R_X86_64_32S
                    (
                        object::RelocationKind::Absolute,
                        object::RelocationEncoding::X86Signed,
                        32,
                    ) => {
                        info!("Relocation type is R_X86_64_32S");
                        // S + A
                        let value = s.wrapping_add(a);
                        output_section.content
                            [(relocation.offset) as usize..(relocation.offset + 4) as usize]
                            .copy_from_slice(&(value as i32).to_le_bytes());
                    }
                    // R_X86_64_PLT32
                    (
                        object::RelocationKind::PltRelative,
                        object::RelocationEncoding::Generic,
                        32,
                    ) => {
                        info!("Relocation type is R_X86_64_PLT32");
                        // we don't have PLT now, implement as R_X86_64_PC32
                        // S + A - P
                        let value = s.wrapping_add(a).wrapping_sub_unsigned(p);

                        output_section.content
                            [(relocation.offset) as usize..(relocation.offset + 4) as usize]
                            .copy_from_slice(&(value as i32).to_le_bytes());
                    }
                    // R_X86_64_PC32
                    (object::RelocationKind::Relative, object::RelocationEncoding::Generic, 32) => {
                        info!("Relocation type is R_X86_64_PC32");
                        // S + A - P
                        let value = s.wrapping_add(a).wrapping_sub_unsigned(p);

                        output_section.content
                            [(relocation.offset) as usize..(relocation.offset + 4) as usize]
                            .copy_from_slice(&(value as i32).to_le_bytes());
                    }
                    _ => unimplemented!("Unimplemented relocation {:?}", relocation),
                }
            }
        }

        Ok(())
    }
}

/// Do the actual linking
pub fn link(opt: &Opt) -> anyhow::Result<()> {
    Linker::link(opt)
}
