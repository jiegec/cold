use crate::opt::{FileOpt, ObjectFileOpt, Opt};
use anyhow::{anyhow, Context};
use log::{info, warn};
use object::elf::Sym64;
use object::write::elf::*;
use object::LittleEndian;
use object::{
    elf::{DT_GNU_HASH, DT_HASH, DT_NULL, DT_SONAME, DT_STRSZ, DT_STRTAB, DT_SYMENT, DT_SYMTAB},
    write::{
        elf::{SectionIndex, Writer},
        StringId,
    },
    Object, ObjectSection, ObjectSymbol,
};
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt, path::PathBuf};
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
    inner: object::Relocation,
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
    // indices in output ELF
    pub section_index: Option<SectionIndex>,
    pub name_string_id: Option<StringId>,
}

struct Linker<'a> {
    opt: Opt,
    files: Vec<ObjectFile>,

    // section name => section
    output_sections: BTreeMap<String, OutputSection>,

    // symbol table: symbol name => symbol
    symbols: BTreeMap<String, Symbol>,

    // section address => offset
    section_address: BTreeMap<String, u64>,

    // elf writer
    writer: Writer<'a>,

    load_address: u64,

    // dynamic, dynsym, dynstr, hash, gnu_hash
    dynamic_section_offset: u64,
    dynsym_section_offset: u64,
    dynstr_section_offset: u64,
    hash_section_offset: u64,
    gnu_hash_section_offset: u64,
    dynamic_entries_count: usize,
    soname_dynamic_string_index: Option<StringId>,
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
            dynamic_section_offset: 0,
            dynamic_entries_count: 0,
            dynsym_section_offset: 0,
            dynstr_section_offset: 0,
            hash_section_offset: 0,
            gnu_hash_section_offset: 0,
            soname_dynamic_string_index: None,
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
            files,
            output_sections,
            symbols,
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
                    let name = format!("{}/{}", file.name, std::str::from_utf8(member.name())?);
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
            match obj {
                object::File::Elf64(elf) => {
                    // collect section sizes prior to this object
                    let section_sizes: BTreeMap<String, u64> = output_sections
                        .iter()
                        .map(|(key, value)| (key.clone(), value.content.len() as u64))
                        .collect();

                    for section in elf.sections() {
                        let name = section.name()?;
                        info!("Handling section {}", name);
                        let data = section.data()?;
                        if data.is_empty() {
                            continue;
                        }

                        if !name.is_empty() {
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
                            out.is_executable |= is_executable;
                            out.is_writable |= is_writable;
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
                                                inner: relocation,
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
                                                inner: relocation,
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
                        if !symbol.is_undefined() && symbol.kind() != object::SymbolKind::Section {
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
                                        },
                                    );
                                }
                                _ => unimplemented!(),
                            }
                        }
                    }
                }
                _ => return Err(anyhow!("Unsupported format of file {}", name)),
            }
        }

        Ok(())
    }

    fn reserve(&mut self, arena: &'a mut Arena<u8>) -> anyhow::Result<()> {
        let Linker {
            opt,
            output_sections,
            symbols,
            writer,
            ..
        } = self;

        // assign address to output sections
        // and generate layout of executable
        // assume executable is loaded at 0x400000
        self.load_address = if opt.shared { 0 } else { 0x400000 };
        // the first page is reserved for ELF header & program header
        writer.reserve_file_header();
        // for simplicity, use one segment to map them all
        writer.reserve_program_headers(if opt.shared {
            // PT_LOAD + PT_DYNAMIC
            2
        } else {
            // PT_LOAD
            1
        });

        // thus sections begin at 0x401000
        for (_name, output_section) in output_sections.iter_mut() {
            output_section.offset = writer.reserve(output_section.content.len(), 4096) as u64;
        }
        info!("Output sections: {:?}", output_sections);

        // reserve section headers
        writer.reserve_null_section_index();
        // use typed-arena to avoid borrow to `output_sections`
        for (name, output_section) in output_sections.iter_mut() {
            output_section.name_string_id =
                Some(writer.add_section_name(arena.alloc_str(name).as_bytes()));
            output_section.section_index = Some(writer.reserve_section_index());
        }
        let _symtab_section_index = writer.reserve_symtab_section_index();
        let _strtab_section_index = writer.reserve_strtab_section_index();
        let _shstrtab_section_index = writer.reserve_shstrtab_section_index();
        if opt.shared {
            // .dynamic, .dynsym, .dynstr, .hash, .gnu_hash
            let _dynamic_section_index = writer.reserve_dynamic_section_index();
            let _dynsym_section_index = writer.reserve_dynsym_section_index();
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
        if opt.shared {
            // dynamic entries:
            // 1. HASH -> .hash
            // 2. GNU_HASH -> .gnu_hash
            // 3. STRTAB -> .dynstr
            // 4. SYMTAB -> .dynsym
            // 5. STRSZ
            // 6. SYMENT
            // 7. SONAME
            // 8. NULL
            if opt.hash_style.sysv {
                self.dynamic_entries_count += 1;
            }
            if opt.hash_style.gnu {
                self.dynamic_entries_count += 1;
            }
            if opt.soname.is_some() {
                self.dynamic_entries_count += 1;
            }

            // align to 8 bytes boundary
            self.dynamic_section_offset = writer.reserve_dynamic(self.dynamic_entries_count) as u64;

            // dynamic symbols
            writer.reserve_null_dynamic_symbol_index();
            let mut dyn_symbols_count = 0;
            for (symbol_name, symbol) in symbols.iter_mut() {
                if symbol.is_global {
                    // only consider global symbols
                    symbol.symbol_name_dynamic_string_id =
                        Some(writer.add_dynamic_string(arena.alloc_str(symbol_name).as_bytes()));
                    writer.reserve_dynamic_symbol_index();
                    dyn_symbols_count += 1;
                }
            }

            if let Some(soname) = &opt.soname {
                self.soname_dynamic_string_index =
                    Some(writer.add_dynamic_string(arena.alloc_str(soname).as_bytes()))
            };

            self.dynsym_section_offset = writer.reserve_dynsym() as u64;

            // dynamic string
            self.dynstr_section_offset = writer.reserve_dynstr() as u64;

            // hash table
            if opt.hash_style.sysv {
                // chain count: 1 extra element for NULL symbol
                self.hash_section_offset =
                    writer.reserve_hash(dyn_symbols_count, dyn_symbols_count + 1) as u64;
            }

            // gnu hash table
            if opt.hash_style.gnu {
                self.gnu_hash_section_offset =
                    writer.reserve_gnu_hash(1, dyn_symbols_count, dyn_symbols_count) as u64;
            }
        };

        Ok(())
    }

    fn write(&mut self) -> anyhow::Result<()> {
        let Linker {
            opt,
            output_sections,
            symbols,
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
        if opt.shared {
            writer.write_program_header(&ProgramHeader {
                p_type: object::elf::PT_DYNAMIC,
                p_flags: object::elf::PF_W | object::elf::PF_R,
                p_offset: self.dynamic_section_offset,
                p_vaddr: self.dynamic_section_offset,
                p_paddr: self.dynamic_section_offset,
                p_filesz: (self.dynamic_entries_count
                    * std::mem::size_of::<object::elf::Dyn64<object::LittleEndian>>())
                    as u64,
                p_memsz: (self.dynamic_entries_count
                    * std::mem::size_of::<object::elf::Dyn64<object::LittleEndian>>())
                    as u64,
                p_align: 8,
            });
        }

        // write section data
        for (_name, output_section) in output_sections.iter() {
            writer.pad_until(output_section.offset as usize);
            writer.write(&output_section.content);
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
                sh_type: object::elf::SHT_PROGBITS,
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
        writer.write_symtab_section_header(
            1 + symbols.iter().filter(|(_name, sym)| !sym.is_global).count() as u32,
        ); // +1: one extra null symbol at the beginning
        writer.write_strtab_section_header();
        writer.write_shstrtab_section_header();
        if opt.shared {
            writer.write_dynamic_section_header(self.dynamic_section_offset);
            writer.write_dynsym_section_header(self.dynsym_section_offset, 1); // one local: null symbol
            writer.write_dynstr_section_header(self.dynstr_section_offset);
            if opt.hash_style.sysv {
                writer.write_hash_section_header(self.hash_section_offset);
            }
            if opt.hash_style.gnu {
                writer.write_gnu_hash_section_header(self.gnu_hash_section_offset);
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
                section: output_sections[&symbol.section_name].section_index,
                st_info: if symbol.is_global {
                    (object::elf::STB_GLOBAL) << 4
                } else {
                    (object::elf::STB_LOCAL) << 4
                },
                st_other: 0,
                st_shndx: 0,
                st_value: address,
                st_size: 0,
            });
        }

        // write string table
        writer.write_strtab();

        // write section string table
        writer.write_shstrtab();

        // shared library
        if opt.shared {
            // dynamic entries:
            // 1. HASH -> .hash
            // 2. GNU_HASH -> .gnu_hash
            // 3. STRTAB -> .dynstr
            // 4. SYMTAB -> .dynsym
            // 5. STRSZ
            // 6. SYMENT
            // 7. SONAME
            // 8. NULL
            writer.write_align_dynamic();
            if opt.hash_style.sysv {
                writer.write_dynamic(DT_HASH, self.hash_section_offset);
            }
            if opt.hash_style.gnu {
                writer.write_dynamic(DT_GNU_HASH, self.gnu_hash_section_offset);
            }
            writer.write_dynamic(DT_STRTAB, self.dynstr_section_offset);
            writer.write_dynamic(DT_SYMTAB, self.dynsym_section_offset);
            let strsz = writer.dynstr_len() as u64;
            writer.write_dynamic(DT_STRSZ, strsz); // size of dynamic string table
            writer.write_dynamic(DT_SYMENT, std::mem::size_of::<Sym64<LittleEndian>>() as u64); // entry size
            if let Some(soname_dynamic_string_index) = &soname_dynamic_string_index {
                writer.write_dynamic_string(DT_SONAME, *soname_dynamic_string_index);
            }
            writer.write_dynamic(DT_NULL, 0);

            // sort symbols by gnu hash bucket: this is required for later gnu hash table to work
            let mut dyn_symbols = vec![];
            for (symbol_name, symbol) in symbols {
                if symbol.is_global {
                    dyn_symbols.push((symbol_name, symbol));
                }
            }
            let bucket_count = dyn_symbols.len();
            dyn_symbols.sort_by_key(|(name, _sym)| {
                let hash = object::elf::gnu_hash(name.as_bytes());
                hash % bucket_count as u32
            });

            // write dynamic symbols
            writer.write_null_dynamic_symbol();
            for (_symbol_name, symbol) in &dyn_symbols {
                let address = section_address[&symbol.section_name] + symbol.offset;
                writer.write_dynamic_symbol(&Sym {
                    name: symbol.symbol_name_dynamic_string_id,
                    section: output_sections[&symbol.section_name].section_index,
                    st_info: (object::elf::STB_GLOBAL) << 4,
                    st_other: 0,
                    st_shndx: 0,
                    st_value: address,
                    st_size: 0,
                });
            }

            // write dynamic string table
            writer.write_dynstr();

            // write hash table
            if opt.hash_style.sysv {
                writer.write_hash(
                    dyn_symbols.len() as u32,
                    dyn_symbols.len() as u32 + 1, // + 1 for NULL symbol at start
                    |idx| {
                        // compute sysv hash of symbol name
                        // 0 is reserved for null, skip
                        if idx == 0 {
                            None
                        } else {
                            Some(object::elf::hash(
                                dyn_symbols[idx as usize - 1].0.as_bytes(),
                            ))
                        }
                    },
                );
            }

            // write gnu hash table
            if opt.hash_style.gnu {
                writer.write_gnu_hash(
                    1, // must be at least one to skip the first NULL symbol
                    1,
                    1,
                    dyn_symbols.len() as u32,
                    dyn_symbols.len() as u32,
                    |idx| {
                        // compute gnu hash of symbol name
                        object::elf::gnu_hash(dyn_symbols[idx as usize].0.as_bytes())
                    },
                );
            }
        }

        Ok(())
    }

    fn relocate(&mut self) -> anyhow::Result<()> {
        let Linker {
            output_sections,
            symbols,
            section_address,
            ..
        } = self;

        // compute mapping from section name to virtual address
        for (name, output_section) in output_sections.iter() {
            section_address.insert(name.clone(), output_section.offset + self.load_address);
        }

        // compute relocation
        for (name, output_section) in output_sections.iter_mut() {
            for relocation in &output_section.relocations {
                info!("Handling relocation {:?} from section {}", relocation, name);
                let target_address = match &relocation.target {
                    RelocationTarget::Section((name, offset)) => {
                        info!("Handling relocation targeting section {}", name);
                        section_address[name] + offset
                    }
                    RelocationTarget::Symbol(name) => {
                        info!("Handling relocation targeting symbol {}", name);
                        let symbol = &symbols[name];
                        section_address[&symbol.section_name] + symbol.offset
                    }
                };

                // symbol
                let s = target_address as i64;
                // addend
                let a = relocation.inner.addend();
                // pc
                let p = self.load_address + output_section.offset + relocation.offset;

                match (
                    relocation.inner.kind(),
                    relocation.inner.encoding(),
                    relocation.inner.size(),
                ) {
                    // R_X86_64_32S
                    (
                        object::RelocationKind::Absolute,
                        object::RelocationEncoding::X86Signed,
                        32,
                    ) => {
                        info!("Handling relocation type R_X86_64_32S");
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
                        info!("Handling relocation type R_X86_64_PLT32");
                        // we don't have PLT now, implement as R_X86_64_PC32
                        // S + A - P
                        let value = s.wrapping_add(a).wrapping_sub_unsigned(p);

                        output_section.content
                            [(relocation.offset) as usize..(relocation.offset + 4) as usize]
                            .copy_from_slice(&(value as i32).to_le_bytes());
                    }
                    // R_X86_64_PC32
                    (object::RelocationKind::Relative, object::RelocationEncoding::Generic, 32) => {
                        info!("Handling relocation type R_X86_64_PC32");
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
