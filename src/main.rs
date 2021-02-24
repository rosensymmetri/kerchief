use clap::{App, Arg, SubCommand};
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::{env, fs, io, path};
use walkdir::WalkDir;

fn find_root() -> anyhow::Result<path::PathBuf> {
    path::Path::new(".")
        .canonicalize()?
        .ancestors()
        .find(|path| path.join("kerchief.toml").is_file())
        .map(path::Path::to_owned)
        .ok_or(anyhow::anyhow!("Found no relevant root"))
}

fn upload_and_submit(store: model::Wall, key: &str, upload_from_dir: &str) -> anyhow::Result<()> {
    let domain = store.get_domain();
    let token = store.get_token();
    let course_id = store.get_course_id()?;
    let assignment_id = store.get_assignment_id(key)?;

    let mut file_ids = Vec::new();
    for entry in WalkDir::new(upload_from_dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .flatten()
    {
        let payload_path = entry.path();
        let payload_name = entry
            .file_name()
            .to_str()
            .ok_or(anyhow::anyhow!("failed to convert payload file name"))?;
        let file_id = canvas::submit_assignment_upload(
            token,
            domain,
            course_id,
            assignment_id,
            payload_path,
            payload_name,
        )?;
        file_ids.push(file_id);
    }

    canvas::submit_assignment_checkout(token, domain, course_id, assignment_id, file_ids)?;

    Ok(())
}

/// The include entries have their transformations applied (as specified by their
/// respective options) and these files are written to a temporary directory (presently
/// the constant path `$KERCHIEF_ROOT/.kerchief/temp`). Returns the directory path.
fn stage_includes(store: &model::Wall, key: &str) -> anyhow::Result<String> {
    let root = find_root()?;
    env::set_current_dir(&root)?;
    let temp = path::Path::new(".kerchief").join("temp");

    // We want to ignore the case where the directory wasn't found, but otherwise pass
    // on the error.
    let res = fs::remove_dir_all(&temp);
    if let Err(e) = res {
        if let io::ErrorKind::NotFound = e.kind() {
            Ok(())
        } else {
            Err(e)
        }
    } else {
        res
    }?;

    fs::create_dir_all(&temp)?;

    for (p, opts) in store.get_assignment_file_paths(key)? {
        if let Ok(include) = p {
            apply_include_transforms(&include, opts.into_iter().flatten().collect(), &temp)?;
        } else if let Err(e) = p {
            println!("{}", e)
        }
    }

    Ok(temp.to_str().unwrap().to_owned())
}

/// Use the settings `opts` to produce the payload for the given `include` entry. The payload
/// is created in the directory `temp`.
fn apply_include_transforms(
    include: &model::IncludePath,
    opts: HashSet<model::FileOption>,
    temp: &path::Path,
) -> anyhow::Result<()> {
    match include {
        model::IncludePath::File(file_path) => {
            if opts.contains(&model::FileOption::Zip) {
                let file_name = file_path.file_name().unwrap().to_str().unwrap();
                let target = temp.join(file_name).with_extension(".zip");
                let target = fs::File::create(target)?;
                let file = fs::read(file_path)?;

                let mut zip = zip::ZipWriter::new(target);
                zip.start_file(file_name, Default::default())?;
                zip.write(&file)?;
                zip.finish()?;
            } else {
                let target = temp.join(file_path.file_name().unwrap());
                // what happens if ´target´ is already taken? possible bug to think about
                let mut target = fs::File::create(target)?;
                let mut file = fs::File::open(file_path)?;

                io::copy(&mut file, &mut target)?;
            }
        }

        model::IncludePath::Dir(dir_path) => {
            if opts.contains(&model::FileOption::Zip) {
                let target = temp.join(dir_path.with_extension("zip").file_name().unwrap());
                let target = fs::File::create(target)?;
                let mut zip = zip::ZipWriter::new(target);

                for entry in WalkDir::new(dir_path)
                    .min_depth(1)
                    .contents_first(false)
                    .into_iter()
                {
                    let entry = entry?;
                    if entry.file_type().is_dir() {
                        zip.add_directory(
                            entry.path().strip_prefix(dir_path)?.to_str().unwrap(),
                            Default::default(),
                        )?;
                    } else if entry.file_type().is_file() {
                        zip.start_file(
                            entry.path().strip_prefix(dir_path)?.to_str().unwrap(),
                            Default::default(),
                        )?;
                        let file = fs::read(entry.path())?;
                        // add buffering dumbfuck
                        zip.write_all(&file)?;
                    }
                    // do nothing with symlinks
                }
                zip.finish()?;
            } else {
                for entry in WalkDir::new(dir_path)
                    .min_depth(1)
                    .into_iter()
                    .filter_entry(|e| e.file_type().is_file())
                {
                    let entry = entry?;
                    let target = temp.join(entry.file_name());
                    // what happens if ´target´ is already taken? possible bug to think about
                    let mut target = fs::File::create(target)?;
                    let mut file = fs::File::open(entry.path())?;

                    io::copy(&mut file, &mut target)?;
                }
            }
        }
    }
    Ok(())
}

fn print_items(temp_dir: &str) -> anyhow::Result<()> {
    for entry in WalkDir::new(temp_dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .flatten()
    {
        println!(
            "    {}",
            entry.path().strip_prefix(temp_dir)?.to_string_lossy()
        );
    }
    Ok(())
}

static CONFIG_TOML_INIT: &str = r#"
token = "<bearer token>"
# Replace <bearer token> by an authorization token for Canvas 
domain = "example.instructure.com"

[course]
name = "Canvas course name"

[assignment.1]
# The name '1' is the local name of the assigment. It is the key that you use 
# for referring to the assignment when submitting.
#
# Example use:
# $ kerchief submit 1
# -- uploads the files in the include paths and bundles them as a submission
#    to the named assignment.

name = "Canvas assignment name"
include = [ "path/to/a/file.txt", "path/to/another/file.txt" ]
"#;

fn main() -> anyhow::Result<()> {
    let matches = App::new("Kerchief")
        .version("0.1-alpha")
        .author("rosensymmetri <o.berndal@gmail.com>")
        .about("Upload assignments to canvas")
        .subcommand(
            SubCommand::with_name("init")
                .about("initialize a `kerchief.toml` configuration file in current directory"),
        )
        .subcommand(
            SubCommand::with_name("submit")
                .about("submit the homework with the given KEY, as specified in `kerchief.toml`")
                .arg(
                    Arg::with_name("key")
                        .value_name("KEY")
                        .required(true)
                        .index(1),
                ),
        )
        .get_matches();
    if let ("init", _) = matches.subcommand() {
        initialize();
    } else if let ("submit", Some(submit_matches)) = matches.subcommand() {
        // key is mandatory argument -> we can unwrap
        let key = submit_matches.value_of("key").unwrap();
        let store = model::Wall::try_from_path("kerchief.toml")?;
        println!("Submit to {}.", store.get_assignment_name(key)?);

        let upload_dir = stage_includes(&store, key)?;
        println!(
            "Preparing to upload the following items (located in {}).",
            &upload_dir
        );
        print_items(&upload_dir)?;
        loop {
            println!("Proceed? (y/n) ");
            let mut line = String::new();
            let stdin = io::stdin();
            stdin.lock().read_line(&mut line)?;
            if line.starts_with(&['y', 'Y'][..]) {
                upload_and_submit(store, key, &upload_dir)?;
                println!("Successful submission.");
                break;
            } else if line.starts_with(&['n', 'N'][..]) {
                println!("Submission cancelled.");
                break;
            }
        }
    }

    Ok(())
}

fn initialize() {
    let response = fs::write("kerchief.toml", CONFIG_TOML_INIT.as_bytes());
    // let response = cfg_file.write();
    match response {
        Ok(_) => println!("Successfully wrote a template configuration to `kerchief.toml`."),
        Err(e) => eprintln!("Failed to write to `kerchief.toml`: {}", e),
    }
}
