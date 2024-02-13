use anyhow::anyhow;
use cold::{LibraryOpt, Opt};
use log::info;

/// handle --push-state/--pop-state
#[derive(Debug, Copy, Clone)]
struct OptStack {
    /// --as-needed
    pub as_needed: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = std::env::args().collect::<Vec<_>>();
    info!("launched with args: {:?}", args);

    // parse arguments
    let mut opt = Opt::default();
    let mut cur_opt_stack = OptStack { as_needed: false };
    let mut opt_stack = vec![];
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-plugin" => {
                // skip plugin argument
                iter.next();
            }
            "-m" => {
                // emulation argument
                opt.emulation = Some(
                    iter.next()
                        .ok_or(anyhow!("Missing emulation after -m"))?
                        .to_string(),
                );
            }
            "-dynamic-linker" => {
                // dynamic linker argument
                opt.dynamic_linker = Some(
                    iter.next()
                        .ok_or(anyhow!("Missing dynamic linker after -dynamic-linker"))?
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
            s @ _ if s.starts_with("-L") => {
                // library search path argument
                opt.search_dir
                    .push(s.strip_prefix("-L").unwrap().to_string());
            }
            s @ _ if s.starts_with("-l") => {
                // library argument
                opt.obj_file.push(cold::ObjFileOpt::Library(LibraryOpt {
                    name: s.strip_prefix("-l").unwrap().to_string(),
                    as_needed: cur_opt_stack.as_needed,
                }));
            }
            "-pie" => {
                opt.pie = true;
            }
            "--as-needed" => {
                cur_opt_stack.as_needed = true;
            }
            "--push-state" => {
                opt_stack.push(cur_opt_stack);
            }
            "--pop-state" => {
                cur_opt_stack = opt_stack.pop().unwrap();
            }
            "--build-id" => {
                opt.build_id = true;
            }
            "--eh-frame-hdr" => {
                opt.eh_frame_hdr = true;
            }
            s @ _ if s.starts_with("-plugin-opt=") => {
                // ignored
            }
            s @ _ if s.starts_with("-") => {
                // unknown flag
                return Err(anyhow!("Unknown argument: {s}"));
            }
            s @ _ => {
                // object file argument
                opt.obj_file.push(cold::ObjFileOpt::File(s.to_string()));
            }
        }
    }

    info!("parsed options: {opt:?}");
    Ok(())
}
