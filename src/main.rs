use clap::{App, Arg, SubCommand};
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::{env, fs, io, path};
use walkdir::WalkDir;

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
                .about("submit the assignment with the given KEY, as specified in `kerchief.toml`")
                .arg(
                    Arg::with_name("key")
                        .value_name("KEY")
                        .required(true)
                        .index(1),
                ),
        )
        .subcommand(
            SubCommand::with_name("view")
                .about("print information about the assignment with the given KEY")
                .arg(
                    Arg::with_name("key")
                        .value_name("KEY|assignments")
                        .required(true)
                        .index(1)
                        .help("KEY is a `kerchief.toml` assignment key"),
                ),
        )
        .get_matches();

    if let ("init", _) = matches.subcommand() {
        initialize()?;
    } else if let ("submit", Some(submit_matches)) = matches.subcommand() {
        // key is mandatory argument -> we can unwrap
        let key = submit_matches.value_of("key").expect("key is mandatory");
        let store = model::Wall::try_from_path("kerchief.toml")?;
        submit(&store, key)?;
    } else if let ("view", Some(submit_matches)) = matches.subcommand() {
        let key = submit_matches.value_of("key").expect("key is mandatory");
        let store = model::Wall::try_from_path("kerchief.toml")?;
        if key == "assignments" {
            view_assignments_printer(&store)?;
        } else {
            view(&store, key)?;
        }
    }

    Ok(())
}

static CONFIG_TOML_INIT: &str = r#"token = "<bearer token>"
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

fn initialize() -> io::Result<()> {
    if !path::Path::new("kerchief.toml").exists() {
        loop {
            println!("`kerchief.toml` already exists, overwrite? (y/n) ");
            let mut line = String::new();
            let stdin = io::stdin();
            stdin.lock().read_line(&mut line)?;
            if line.starts_with(&['y', 'Y'][..]) {
                break;
            } else if line.starts_with(&['n', 'N'][..]) {
                println!("Cancelled.");
                return Ok(());
            }
        }
    }
    let response = fs::write("kerchief.toml", CONFIG_TOML_INIT.as_bytes());
    // let response = cfg_file.write();
    match response {
        Ok(_) => println!("Successfully wrote a template configuration to `kerchief.toml`."),
        Err(e) => eprintln!("Failed to write to `kerchief.toml`: {}", e),
    }
    Ok(())
}

fn submit(store: &model::Wall, key: &str) -> anyhow::Result<()> {
    println!("Submit to {}.", store.get_assignment(key)?.name());

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
            break Ok(());
        } else if line.starts_with(&['n', 'N'][..]) {
            println!("Submission cancelled.");
            break Ok(());
        }
    }
}

fn view(store: &model::Wall, key: &str) -> anyhow::Result<()> {
    let assignment = store.get_assignment(key)?;
    let submission = store.get_latest_submission(assignment.id())?;
    print_assignment(assignment, submission.as_ref());
    Ok(())
}

fn view_assignments(store: &model::Wall) -> anyhow::Result<()> {
    for a in store.iter_assignments()? {
        print_assignment(a, store.get_latest_submission(a.id())?.as_ref());
    }
    Ok(())
}

fn view_assignments_printer(store: &model::Wall) -> anyhow::Result<()> {
    let mut printer = display::Printer::new(80);
    let mut assignment_list = display::ItemList::new(0, 0, 20).prefixes("┓ ", "  ", "┗ ", "┻ ");
    let mut assignment_dues = display::ItemList::new(18, 0, 36);
    let mut assignment_duesd = Vec::new();

    for a in store.iter_assignments()? {
        assignment_list.add_item(a.name());
        assignment_duesd.push(a.due_at().to_rfc2822());
    }

    for d in assignment_duesd.iter() {
        assignment_dues.add_item(d);
    }

    assignment_dues.write_to_printer(&mut printer);
    assignment_list.write_to_printer(&mut printer);
    printer.print();
    Ok(())
}

fn print_assignment(assignment: &canvas::Assignment, submission: Option<&canvas::Submission>) {
    let name = assignment.name();
    let due_at = assignment.due_at();
    let lock_at = assignment.lock_at();
    let unlock_at = assignment.unlock_at();

    let mut printer = display::Printer::new(100);
    let mut title = display::ItemList::new(0, 0, 20).prefix("┓ ");
    title.add_item(name);

    let mut dates = display::ItemList::new(22, 0, 36);
    dates.add_item(&format!(" due {}", due_at));
    if let Some(lock_at) = lock_at {
        dates.add_item(&format!("lock {}", lock_at));
    }
    if let Some(unlock_at) = unlock_at {
        dates.add_item(&format!("open {}", unlock_at));
    }

    if let Some(submission) = submission {
        print!("  last submitted {}", submission.submitted_at());
        if let canvas::SubmissionTypeResponse::OnlineUpload { attachments } =
            submission.submission_type_response()
        {
            print!(", files");
            for a in attachments.iter() {
                print!(" {}", a.filename());
            }
        }
        println!("");
    }
}

fn find_root() -> anyhow::Result<path::PathBuf> {
    path::Path::new(".")
        .canonicalize()?
        .ancestors()
        .find(|path| path.join("kerchief.toml").is_file())
        .map(path::Path::to_owned)
        .ok_or(anyhow::anyhow!("Found no relevant root"))
}

fn upload_and_submit(store: &model::Wall, key: &str, upload_from_dir: &str) -> anyhow::Result<()> {
    let domain = store.get_domain();
    let token = store.get_token();
    let course_id = store.get_course_id()?;
    let assignment_id = store.get_assignment(key)?.id();

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

mod display {

    pub struct Printer {
        width: u8,
        lines: Vec<Vec<(u8, String)>>,
    }

    impl Printer {
        pub fn new(width: u8) -> Self {
            let lines = Vec::new();
            Self { width, lines }
        }

        fn get_line(&mut self, idx: usize) -> &mut Vec<(u8, String)> {
            let n = self.lines.len();
            if n > idx {
                self.lines.get_mut(idx).unwrap()
            } else {
                for _ in 0..(idx - n + 1) {
                    self.lines.push(Vec::new());
                }
                self.lines.last_mut().unwrap()
            }
        }

        fn insert_on_line(&mut self, idx: usize, load: (u8, &str)) {
            self.get_line(idx).push((load.0, load.1.to_owned()));
            self.get_line(idx).sort_by(|(o1, _), (o2, _)| o1.cmp(o2));
        }

        fn transform_line_for_print(&self, line: Vec<(u8, String)>) -> String {
            let mut out_chars: usize = self.width as usize;
            let mut out = String::new();
            for _ in 0..self.width {
                out.push(' ');
            }

            for (offset, content) in line.into_iter().filter(|&(offset, _)| offset < self.width) {
                Self::insert_with_filled_offset(
                    &mut out,
                    &mut out_chars,
                    offset as usize,
                    &content,
                );
            }

            // Make sure that the final string has correct size by inserting an
            // empty string at desired end
            Self::insert_with_filled_offset(&mut out, &mut out_chars, self.width as usize, "");
            out
        }

        /// Inserts `content` such that there precisely are `offset` preceding
        /// characters. Excess characters are discarded, if too few characters
        /// then we fill up with `' '`.
        /// Risks panicking if `buf_chars` is not the number of chars in `buf`.
        fn insert_with_filled_offset(
            buf: &mut String,
            buf_chars: &mut usize,
            offset: usize,
            content: &str,
        ) {
            if *buf_chars < offset {
                for _ in 0..(offset - *buf_chars) {
                    buf.push(' ');
                    *buf_chars += 1;
                }
            } else {
                for _ in 0..(*buf_chars - offset) {
                    buf.pop();
                    *buf_chars -= 1;
                }
            }
            buf.push_str(content);
            *buf_chars += content.chars().count();
        }

        pub fn print(&mut self) {
            let mut lines = Vec::new();
            std::mem::swap(&mut lines, &mut self.lines);
            for line in lines.into_iter() {
                let output = self.transform_line_for_print(line);
                println!("{}", output);
            }
        }
    }

    pub struct TitledItemList<'data, 'style> {
        x: u8,
        y: u8,
        dx: u8,
        title: &'data str,
        title_prefix: &'style str,
        children: Vec<&'data str>,
        child_indent: u8,
        child_prefix_first: &'style str,
        child_prefix_middle: &'style str,
        child_prefix_last: &'style str,
        child_prefix_sole: &'style str,
        overfill_symbol: &'style str,
    }

    impl<'data, 'style> TitledItemList<'data, 'style> {
        pub fn write_to_printer(&self, printer: &mut Printer) {
            let mut x = self.x;
            let mut line_offset = self.y;

            // This closure needs to be mutable because we are mutating a non-
            // bound variable (printer)?
            let mut send_to_printer = |prefix: &'style str, content: &'data str, mut indent| {
                // We probably should have these calculated before we get here.
                let content_width = content.chars().count() as u8;
                let prefix_width = prefix.chars().count() as u8;

                let mut prefixed_content = prefix.to_owned();
                prefixed_content.push_str(content);
                let mut lines = prefixed_content.lines();

                let first_line = lines.next().unwrap();
                printer.insert_on_line(line_offset as usize, (indent, first_line));
                if prefix_width + content_width > self.dx {
                    printer.insert_on_line(
                        line_offset as usize,
                        (indent + self.dx - 1, self.overfill_symbol),
                    );
                }
                indent += prefix_width;
                line_offset += 1;
                for line in lines {
                    printer.insert_on_line(line_offset as usize, (indent, line));
                    if line.chars().count() as u8 > self.dx {
                        printer.insert_on_line(
                            line_offset as usize,
                            (indent + self.dx - 1, self.overfill_symbol),
                        );
                    }
                    line_offset += 1;
                }
            };

            send_to_printer(self.title_prefix, self.title, x);

            x += self.child_indent;
            let n = self.children.len();
            if n == 1 {
                send_to_printer(self.child_prefix_sole, self.children[0], x);
            } else if n > 1 {
                send_to_printer(self.child_prefix_first, self.children[0], x);
                for child in self.children[1..n - 1].iter() {
                    send_to_printer(self.child_prefix_middle, child, x);
                }
                send_to_printer(self.child_prefix_last, self.children[n - 1], x);
            }
        }

        pub fn new(x: u8, y: u8, dx: u8) -> Self {
            Self {
                x,
                y,
                dx,
                title: "",
                title_prefix: "",
                children: Vec::new(),
                child_indent: 0,
                child_prefix_first: "",
                child_prefix_middle: "",
                child_prefix_last: "",
                child_prefix_sole: "",
                overfill_symbol: "\u{2591}",
            }
        }

        pub fn title(mut self, title: &'data str) -> Self {
            self.title = title;
            self
        }

        pub fn title_prefix(mut self, prefix: &'style str) -> Self {
            self.title_prefix = prefix;
            self
        }

        pub fn child_prefixes(
            mut self,
            first: &'style str,
            middle: &'style str,
            last: &'style str,
            sole: &'style str,
        ) -> Self {
            self.child_prefix_first = first;
            self.child_prefix_middle = middle;
            self.child_prefix_last = last;
            self.child_prefix_sole = sole;
            self
        }

        pub fn child_indent(mut self, indent: u8) -> Self {
            self.child_indent = indent;
            self
        }

        pub fn add_child(&mut self, child: &'data str) {
            self.children.push(child);
        }
    }

    #[derive(Copy, Clone, Debug)]
    enum BreakToken<'a> {
        Line,
        Token(&'a str),
    }

    impl<'a> BreakToken<'a> {
        fn width(&self) -> usize {
            match self {
                Self::Line => 0,
                Self::Token(s) => s.chars().count(),
            }
        }

        fn end_width(&self) -> usize {
            match self {
                Self::Line => 0,
                Self::Token(s) => s.trim_end().chars().count(),
            }
        }

        fn line() -> Self {
            Self::Line
        }

        fn token(s: &'a str) -> Self {
            Self::Token(s)
        }
    }

    struct LineTokenIterator<'a> {
        base: &'a str,
        has_started: bool,
    }

    impl<'a> LineTokenIterator<'a> {
        fn new(base: &'a str) -> Self {
            Self {
                base,
                has_started: false,
            }
        }
    }

    impl<'a> Iterator for LineTokenIterator<'a> {
        type Item = BreakToken<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            if self.has_started && self.base.is_empty() {
                None
            } else {
                self.has_started = true;
                if let Some(idx) = self.base.find('\n') {
                    if idx == 0 {
                        self.base = &self.base['\n'.len_utf8()..];
                        Some(Self::Item::line())
                    } else {
                        let item = Self::Item::token(&self.base[..idx]);
                        self.base = &self.base[idx..];
                        Some(item)
                    }
                } else {
                    let item = Self::Item::token(&self.base[..]);
                    self.base = "";
                    Some(item)
                }
            }
        }
    }

    trait TokenizeLines<'a> {
        fn tokenize_lines(self) -> LineTokenIterator<'a>;
    }

    impl<'a> TokenizeLines<'a> for &'a str {
        fn tokenize_lines(self) -> LineTokenIterator<'a> {
            LineTokenIterator::new(self)
        }
    }

    struct BreakSeparatorsIterator<'a, I> {
        iter: I,
        carry: Option<&'a str>,
        separators: Vec<char>,
    }

    impl<'a, I> BreakSeparatorsIterator<'a, I> {
        fn new(iter: I, separators: Vec<char>) -> Self {
            Self {
                iter,
                carry: None,
                separators,
            }
        }

        /// Parse the initial part of `s`, and add any leftover to `self.carry`.
        /// Returns initial part with an eventual separator at the end included.
        fn advance(&mut self, s: &'a str) -> &'a str {
            if let Some(char_start) = s.find(&self.separators[..]) {
                // As we found a char beginning at idx, the unwrap never panics.
                let c = s[char_start..].chars().next().unwrap();
                let char_end = char_start + c.len_utf8();

                self.carry = Some(&s[char_end..]);
                &s[..char_end]
            } else {
                self.carry = None;
                &s[..]
            }
        }
    }

    impl<'a, I> Iterator for BreakSeparatorsIterator<'a, I>
    where
        I: Iterator<Item = BreakToken<'a>>,
    {
        type Item = BreakToken<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            if let Some(s) = self.carry {
                self.carry = None;
                Some(Self::Item::token(self.advance(s)))
            } else {
                if let Some(t) = self.iter.next() {
                    match t {
                        Self::Item::Line => Some(Self::Item::line()),
                        Self::Item::Token(s) => Some(Self::Item::token(self.advance(s))),
                    }
                } else {
                    None
                }
            }
        }
    }

    trait BreakAtSep<'a>
    where
        Self: Sized,
    {
        fn split_at_separators(self, separators: &[char]) -> BreakSeparatorsIterator<'a, Self>;
    }

    impl<'a, I> BreakAtSep<'a> for I
    where
        I: Iterator<Item = BreakToken<'a>> + Sized,
    {
        fn split_at_separators(self, separators: &[char]) -> BreakSeparatorsIterator<'a, Self> {
            BreakSeparatorsIterator::new(self, separators.to_owned())
        }
    }

    struct InsertLinesIterator<'a, I> {
        iter: I,
        max_width: usize,
        current_width: usize,
        carry: Option<BreakToken<'a>>,
    }

    impl<'a, I> InsertLinesIterator<'a, I> {
        fn new(iter: I, max_width: usize) -> Self {
            Self {
                iter,
                max_width,
                current_width: 0,
                carry: None,
            }
        }

        fn fits(&self, t: &BreakToken<'a>) -> bool {
            self.current_width + t.end_width() < self.max_width
        }

        fn advance(&mut self, t: BreakToken<'a>) -> BreakToken<'a> {
            match t {
                BreakToken::Line => {
                    self.current_width = 0;
                    BreakToken::line()
                }
                BreakToken::Token(s) => {
                    self.current_width += s.len();
                    BreakToken::token(s)
                }
            }
        }
    }

    impl<'a, I> Iterator for InsertLinesIterator<'a, I>
    where
        I: Iterator<Item = BreakToken<'a>>,
    {
        type Item = BreakToken<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            if let Some(t) = self.carry {
                self.carry = None;
                Some(self.advance(t))
            } else if let Some(t) = self.iter.next() {
                if self.fits(&t) {
                    Some(self.advance(t))
                } else {
                    self.carry = Some(t);
                    Some(self.advance(Self::Item::line()))
                }
            } else {
                None
            }
        }
    }

    trait WrapLines<'a>
    where
        Self: Sized,
    {
        fn wrap_lines(self, max_width: usize) -> InsertLinesIterator<'a, Self>;
    }

    impl<'a, I> WrapLines<'a> for I
    where
        I: Iterator<Item = BreakToken<'a>> + Sized,
    {
        fn wrap_lines(self, max_width: usize) -> InsertLinesIterator<'a, Self> {
            InsertLinesIterator::new(self, max_width)
        }
    }

    impl<'a> std::iter::FromIterator<BreakToken<'a>> for String {
        fn from_iter<T: IntoIterator<Item = BreakToken<'a>>>(iter: T) -> Self {
            let mut out = String::new();
            for token in iter.into_iter() {
                match token {
                    T::Item::Line => out.push('\n'),
                    T::Item::Token(s) => out.push_str(s),
                }
            }
            out
        }
    }

    /// The x, y coordinates are for position and dx, dy are for size.
    /// """
    /// o o o o
    /// o o o o
    /// o x x o
    /// o x x o
    /// """
    /// The `x`'s correspond to `x, y = 1, 2` and `dx = 1`. Notice that `dy` is
    /// unspecified.
    pub struct ItemList<'data, 'style> {
        x: u8,
        y: u8,
        dx: u8,
        items: Vec<&'data str>,
        prefix_first: &'style str,
        prefix_middle: &'style str,
        prefix_last: &'style str,
        prefix_sole: &'style str,
        overfill_symbol: &'style str,
        separators: Vec<char>,
        wrap_lines: bool,
    }

    impl<'data, 'style> ItemList<'data, 'style> {
        pub fn write_to_printer(&self, printer: &mut Printer) -> usize {
            let indent = self.x;
            let mut line_offset = self.y;

            // This closure needs to be mutable because we are mutating a non-
            // bound variable (printer)?
            let mut send_to_printer = |prefix: &'style str, content: &'data str| -> usize {
                // We probably should have these calculated before we get here.
                let content_width = content.chars().count() as u8;
                let prefix_width = prefix.chars().count() as u8;

                let content: String = content
                    .tokenize_lines()
                    .split_at_separators(&self.separators)
                    .wrap_lines((self.dx - prefix_width) as usize)
                    .collect();
                let mut dy = 0;
                let mut lines = content.lines().inspect(|_| dy += 1);

                let first_line = lines.next().unwrap();
                printer.insert_on_line(line_offset as usize, (indent, prefix));
                printer.insert_on_line(line_offset as usize, (indent + prefix_width, first_line));
                if prefix_width + content_width > self.dx {
                    printer.insert_on_line(
                        line_offset as usize,
                        (indent + self.dx - 1, self.overfill_symbol),
                    );
                }
                line_offset += 1;
                for line in lines {
                    printer.insert_on_line(line_offset as usize, (indent, line));
                    if prefix_width + line.chars().count() as u8 > self.dx {
                        printer.insert_on_line(
                            line_offset as usize,
                            (indent + self.dx - 1, self.overfill_symbol),
                        );
                    }
                    line_offset += 1;
                }
                dy
            };

            let n = self.items.len();
            let mut dy = 0;
            if n == 1 {
                dy += send_to_printer(self.prefix_sole, self.items[0]);
            } else if n > 1 {
                dy += send_to_printer(self.prefix_first, self.items[0]);
                for item in self.items[1..n - 1].iter() {
                    dy += send_to_printer(self.prefix_middle, item);
                }
                dy += send_to_printer(self.prefix_last, self.items[n - 1]);
            }
            dy
        }

        pub fn new(x: u8, y: u8, dx: u8) -> Self {
            Self {
                x,
                y,
                dx,
                items: Vec::new(),
                prefix_first: "",
                prefix_middle: "",
                prefix_last: "",
                prefix_sole: "",
                overfill_symbol: "▒",
                separators: vec![' ', '/', '\\', ',', '.'],
                wrap_lines: false,
            }
        }

        pub fn prefix(mut self, pre: &'style str) -> Self {
            self.prefix_first = pre;
            self.prefix_middle = pre;
            self.prefix_last = pre;
            self.prefix_sole = pre;
            self
        }

        pub fn prefixes(
            mut self,
            first: &'style str,
            middle: &'style str,
            last: &'style str,
            sole: &'style str,
        ) -> Self {
            self.prefix_first = first;
            self.prefix_middle = middle;
            self.prefix_last = last;
            self.prefix_sole = sole;
            self
        }

        pub fn add_item(&mut self, child: &'data str) {
            self.items.push(child);
        }

        pub fn wrap_lines(mut self) -> Self {
            self.wrap_lines = true;
            self
        }

        pub fn overfill_symbol(mut self, symbol: &'style str) -> Self {
            self.overfill_symbol = symbol;
            self
        }
    }

    pub struct TitledList<'data, 'style> {
        x: u8,
        y: u8,
        dx: u8,
        title: &'data str,
        title_prefix: &'style str,
        children: Vec<&'data str>,
        child_indent: u8,
        child_prefix_first: &'style str,
        child_prefix_middle: &'style str,
        child_prefix_last: &'style str,
        child_prefix_sole: &'style str,
        separators: Vec<char>,
        wrap_title: bool,
        wrap_children: bool,
        overfill_symbol: &'style str,
    }

    impl<'data, 'style> TitledList<'data, 'style> {
        pub fn new(x: u8, y: u8, dx: u8) -> Self {
            Self {
                x,
                y,
                dx,
                title: "",
                title_prefix: "",
                children: Vec::new(),
                child_indent: 0,
                child_prefix_first: "",
                child_prefix_middle: "",
                child_prefix_last: "",
                child_prefix_sole: "",
                separators: Vec::new(),
                wrap_title: false,
                wrap_children: false,
                overfill_symbol: "▒",
            }
        }

        pub fn title(mut self, title: &'data str) -> Self {
            self.title = title;
            self
        }

        pub fn title_prefix(mut self, title_prefix: &'style str) -> Self {
            self.title_prefix = title_prefix;
            self
        }

        pub fn child_prefixes(
            mut self,
            first: &'style str,
            middle: &'style str,
            last: &'style str,
            sole: &'style str,
        ) -> Self {
            self.child_prefix_first = first;
            self.child_prefix_middle = middle;
            self.child_prefix_last = last;
            self.child_prefix_sole = sole;
            self
        }

        pub fn wrap_children(mut self) -> Self {
            self.wrap_children = true;
            self
        }

        pub fn add_child(&mut self, child: &'data str) {
            self.children.push(child);
        }

        pub fn separators(mut self, separators: Vec<char>) -> Self {
            self.separators = separators;
            self
        }

        pub fn child_indent(mut self, indent: u8) -> Self {
            self.child_indent = indent;
            self
        }

        pub fn write_to_printer(&self, printer: &mut Printer) {
            let mut itemlist_title = ItemList::new(self.x, self.y, self.dx)
                .prefixes(
                    self.title_prefix,
                    self.title_prefix,
                    self.title_prefix,
                    self.title_prefix,
                )
                .overfill_symbol(self.overfill_symbol);
            itemlist_title.add_item(self.title);

            let mut itemlist_children = ItemList::new(
                self.x + self.child_indent,
                self.y + 1,
                self.dx - self.child_indent,
            )
            .overfill_symbol(self.overfill_symbol)
            .wrap_lines()
            .prefixes(
                self.child_prefix_first,
                self.child_prefix_middle,
                self.child_prefix_last,
                self.child_prefix_sole,
            );
            for item in self.children.iter() {
                itemlist_children.add_item(item);
            }

            itemlist_title.write_to_printer(printer);
            itemlist_children.write_to_printer(printer);
        }
    }
}
