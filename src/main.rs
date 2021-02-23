mod canvas {
    use reqwest::blocking::multipart::Form;
    use reqwest::blocking::Client;
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::path::Path;
    use thiserror::Error;

    #[derive(Deserialize, Debug)]
    pub struct FileUploadEntry {
        upload_url: String,
        upload_params: HashMap<String, String>,
    }

    #[derive(Deserialize, Debug)]
    pub struct FileUploadResponse {
        id: u64,
    }

    // pub mod payload {
    //     use std::{io, path};
    //     use thiserror::Error;

    //     pub struct Payload<'a> {
    //         path: &'a path::Path,
    //         name: &'a str,
    //         size: u64,
    //     }

    //     #[derive(Error, Debug)]
    //     pub enum PayloadPathError {
    //         #[error("unable to construct name from file")]
    //         NoName,
    //         #[error("the path is not a file")]
    //         NotFile(path::PathBuf),
    //         #[error("failed to gather metadata for the file: {0:?}")]
    //         FileSystem(#[from] io::Error),
    //     }

    //     impl<'a> From<path::PathBuf> for PayloadPathError {
    //         fn from(p: path::PathBuf) -> Self {
    //             Self::NotFile(p)
    //         }
    //     }

    //     impl<'a> Payload<'a> {
    //         pub fn path(&self) -> &path::Path {
    //             &self.path
    //         }

    //         pub fn name(&self) -> &str {
    //             self.name
    //         }

    //         pub fn size(&self) -> u64 {
    //             self.size
    //         }
    //     }

    //     use std::convert;
    //     impl<'a> convert::TryFrom<&'a str> for Payload<'a> {
    //         type Error = PayloadPathError;

    //         fn try_from(path: &'a str) -> Result<Self, Self::Error> {
    //             let path = path::Path::new(path).canonicalize()?;

    //             let metadata = path.metadata()?;

    //             if !metadata.is_file() {
    //                 return Err(PayloadPathError::from(path));
    //             }

    //             let name = path
    //                 .file_name()
    //                 .ok_or(PayloadPathError::NoName)?
    //                 .to_str()
    //                 .ok_or(PayloadPathError::NoName)?;

    //             let size = metadata.len();

    //             Ok(Payload {
    //                 path: &path,
    //                 name,
    //                 size,
    //             })
    //         }
    //     }
    // }

    #[derive(Error, Debug)]
    pub enum Error {
        #[error("location header missing in redirect response")]
        NoRedirectLocation,
        #[error(transparent)]
        ToString(#[from] reqwest::header::ToStrError),
        #[error(transparent)]
        FileSystem(#[from] std::io::Error),
        #[error(transparent)]
        Reqwest(#[from] reqwest::Error),
    }

    pub type Result<T> = std::result::Result<T, Error>;

    pub fn submit_assignment_upload<P: AsRef<Path>>(
        auth: &str,
        domain: &str,
        course_id: u64,
        assignment_id: u64,
        payload_path: P,
        payload_name: &str,
    ) -> Result<u64> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        let payload_data = payload_path.as_ref().metadata()?;

        let entry = client
            .post(&format!(
                "https://{}/api/v1/courses/{}/assignments/{}/submissions/self/files",
                domain, course_id, assignment_id
            ))
            .query(&[("name", payload_name)])
            .query(&[("size", payload_data.len())])
            .bearer_auth(auth)
            .send()?
            .error_for_status()?
            .json::<FileUploadEntry>()?;

        let (url, params) = (entry.upload_url, entry.upload_params);

        let mut form = Form::new();
        for (key, val) in params.into_iter() {
            form = form.text(key, val);
        }
        form = form.file("file", payload_path)?;

        let mut upload = client
            .post(&url)
            .multipart(form)
            .send()?
            .error_for_status()?;

        if upload.status().is_redirection() {
            let redirect_url = upload
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| Error::NoRedirectLocation)?
                .to_str()?;
            upload = client
                .get(redirect_url)
                .bearer_auth(auth)
                .send()?
                .error_for_status()?;
        };

        Ok(upload.json::<FileUploadResponse>()?.id)
    }

    /// After uploading the files, we need to confirm that they shall be included in a submission.
    /// The parameter `file_ids` contains the `file_id` of each uploaded file to be included.
    pub fn submit_assignment_checkout(
        auth: &str,
        domain: &str,
        course_id: u64,
        assignment_id: u64,
        file_ids: Vec<u64>,
    ) -> Result<()> {
        let client = reqwest::blocking::Client::new();

        let file_ids_query = file_ids
            .into_iter()
            .map(|id| ("submission[file_ids][]", id))
            .collect::<Vec<(&str, u64)>>();

        let _submit = client
            .post(&format!(
                "https://{}/api/v1/courses/{}/assignments/{}/submissions",
                domain, course_id, assignment_id
            ))
            .query(&[("submission[submission_type]", "online_upload")])
            .query(&file_ids_query)
            .bearer_auth(auth)
            .send()?
            .error_for_status()?;
        // logic about how to handle requests where the response indicates that something bad happened
        // should maybe go here

        Ok(())
    }

    #[derive(Clone, Deserialize, Debug)]
    pub struct Course {
        id: u64,
        name: String,
    }

    impl Course {
        pub fn id(&self) -> u64 {
            self.id
        }

        pub fn name(&self) -> &str {
            &self.name
        }
    }

    /// Returns the `id` and `name` of each course associated with the `auth` token.
    pub fn get_courses(token: &str, domain: &str) -> Result<Vec<Course>> {
        Ok(Client::new()
            .get(&format!("https://{}/api/v1/courses", domain))
            .bearer_auth(token)
            .send()?
            .error_for_status()?
            .json()?)
    }

    #[derive(Clone, Deserialize, Debug)]
    pub struct Assignment {
        id: u64,
        name: String,
    }

    impl Assignment {
        pub fn name(&self) -> &str {
            &self.name
        }

        pub fn id(&self) -> u64 {
            self.id
        }
    }

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum Bucket {
        Past,
        Overdue,
        Undated,
        Ungraded,
        Unsubmitted,
        Upcoming,
        Future,
    }

    pub fn get_assignments(token: &str, domain: &str, course_id: u64) -> Result<Vec<Assignment>> {
        Ok(Client::new()
            .get(&format!(
                "https://{}/api/v1/courses/{}/assignments",
                domain, course_id
            ))
            .bearer_auth(token)
            .send()?
            .error_for_status()?
            .json()?)
    }
}

mod cfg {
    use serde::Deserialize;
    use std::borrow::Borrow;
    use std::collections::HashMap;
    use thiserror::Error;

    #[derive(Deserialize, Debug, PartialEq)]
    pub struct Config {
        token: String,
        domain: String,
        course: Identifier,
        assignment: HashMap<String, Assignment>,
    }

    #[derive(Debug, Error)]
    pub enum FetchError {
        #[error("The assignment key '{0}' is not present in the configuration.")]
        NoSuchAssignmentKey(String),
    }

    impl Config {
        pub fn token(&self) -> &str {
            &self.token
        }

        pub fn domain(&self) -> &str {
            &self.domain
        }

        pub fn course_ident(&self) -> &Identifier {
            &self.course
        }

        fn assignments(&self) -> impl Iterator<Item = (&str, &Assignment)> + '_ {
            self.assignment.iter().map(|(key, a)| (key.borrow(), a))
        }

        pub fn assignment(&self, key: &str) -> Result<&Assignment, FetchError> {
            match self.assignments().find(|(k, _)| key == *k).map(|(_, a)| a) {
                Some(assignment) => Ok(assignment),
                None => Err(FetchError::NoSuchAssignmentKey(key.to_owned())),
            }
        }
    }

    #[derive(Clone, Deserialize, Debug, PartialEq, Default)]
    pub struct Identifier {
        pub(super) name: Option<String>,
        pub(super) id: Option<u64>,
    }

    pub enum ReadIdentifier<'read> {
        NameAndId { name: &'read str, id: u64 },
        NameOnly { name: &'read str },
        IdOnly { id: u64 },
        None,
    }

    impl Identifier {
        pub fn is_none(&self) -> bool {
            self.name.is_none() && self.name.is_none()
        }

        pub fn read(&self) -> ReadIdentifier<'_> {
            match self {
                Identifier {
                    name: Some(name),
                    id: Some(id),
                } => {
                    let id = *id;
                    ReadIdentifier::NameAndId { name, id }
                }
                Identifier {
                    name: Some(name),
                    id: None,
                } => ReadIdentifier::NameOnly { name },
                Identifier {
                    name: None,
                    id: Some(id),
                } => {
                    let id = *id;
                    ReadIdentifier::IdOnly { id }
                }
                Identifier {
                    name: None,
                    id: None,
                } => ReadIdentifier::None,
            }
        }
    }

    #[derive(Deserialize, Debug, PartialEq)]
    pub struct Assignment {
        #[serde(flatten)]
        ident: Identifier,
        include: Include,
    }

    impl Assignment {
        pub fn ident(&self) -> &Identifier {
            &self.ident
        }

        pub fn include(&self) -> Vec<&Path> {
            match &self.include {
                Include::Single(path) => vec![path],
                Include::Many(paths) => paths.iter().collect(),
            }
        }
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(untagged)]
    pub enum Path {
        Flat(String),
        Optioned {
            path: String,
            options: Option<Vec<String>>,
        },
    }

    use std::collections::HashSet;

    impl Path {
        pub fn path(&self) -> &str {
            match self {
                Self::Flat(path) => &path,
                Self::Optioned { path, .. } => &path,
            }
        }

        pub fn options(&self) -> HashSet<&str> {
            match self {
                Self::Flat(_) => HashSet::new(),
                Self::Optioned { options, .. } => options
                    .iter()
                    .map(|s| s.iter())
                    .flatten()
                    .map(String::as_ref)
                    .collect(),
            }
        }
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(untagged)]
    pub(super) enum Include {
        Single(Path),
        Many(Vec<Path>),
    }

    #[test]
    fn simple_parsing_example() {
        let s = String::from;

        let mut config = Config {
            token: s("1234"),
            domain: s("uppsala.instructure.com"),
            course: Identifier {
                name: Some(s("Datorgrafik")),
                id: None,
            },
            assignment: HashMap::new(),
        };

        config.assignment.insert(
            s("1"),
            Assignment {
                ident: Identifier {
                    name: Some(s("Assignment 1")),
                    id: None,
                },
                include: Include::Many(vec![
                    Path::Flat(s("group.txt")),
                    Path::Optioned {
                        path: s("assignment1"),
                        options: Some(vec![s("zip")]),
                    },
                ]),
            },
        );

        let parse_toml: Config = toml::from_str(
            r##"
token = "1234"
domain = "uppsala.instructure.com"

[course]
name = "Datorgrafik"
# id = 23838

[assignment.1]
name = "Assignment 1"
include = [ "group.txt",
            { path = "assignment1", options = ["zip"] }, ]
"##,
        )
        .expect("ought to be valid toml");

        assert_eq!(parse_toml, config);
    }
}

mod model {
    use crate::{canvas, cfg};
    use once_cell::unsync::OnceCell;
    use std::convert::TryFrom;
    use std::{fmt, path};
    use thiserror::Error;

    #[derive(Clone, Debug, Hash, PartialEq, Eq)]
    pub enum FileOption {
        Zip,
    }

    #[derive(Clone, Debug, Error)]
    pub enum FileOptionError {
        #[error("{0}")]
        Unexpected(String),
    }

    impl TryFrom<&str> for FileOption {
        type Error = FileOptionError;
        fn try_from(string: &str) -> Result<Self, Self::Error> {
            if string == "zip" {
                Ok(FileOption::Zip)
            } else {
                Err(FileOptionError::Unexpected(string.to_owned()))
            }
        }
    }

    impl fmt::Display for FileOption {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let display = match &self {
                FileOption::Zip => "zip",
            };

            write!(f, "{}", display)
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct OwnedIdentifier {
        id: u64,
        name: String,
    }

    impl OwnedIdentifier {
        pub fn id(&self) -> u64 {
            self.id
        }
    }

    #[derive(Clone, Debug, Error)]
    pub enum IdentifierError {
        #[error("fields 'id', 'name' missing from config, either should be sufficient")]
        NotSpecified { alternatives: Vec<OwnedIdentifier> },
        #[error(
            "the name '{user_provided}' is not sufficient to distinguish among possible values"
        )]
        UnderSpecified {
            user_provided: String,
            alternatives: Vec<OwnedIdentifier>,
        },
        #[error(
            "the user provided identifier matches some canvas identifier by 'id' but not by 'name'"
        )]
        NameConflict {
            user_provided: OwnedIdentifier,
            match_id_not_name: Vec<OwnedIdentifier>,
        },
        #[error(
            "the user provided identifier matches some canvas identifier by 'name' but not by 'id'"
        )]
        IdConflict {
            user_provided: OwnedIdentifier,
            match_name_not_id: Vec<OwnedIdentifier>,
        },
        #[error("the id '{user_provided}' is not present among possible values")]
        NoSuchId {
            user_provided: u64,
            alternatives: Vec<OwnedIdentifier>,
        },
        #[error("the name '{user_provided}' is not present among possible values")]
        NoSuchName {
            user_provided: String,
            alternatives: Vec<OwnedIdentifier>,
        },
        #[error("the user provided identifier is not present among possible values")]
        NoSuchIdentifier {
            user_provided: OwnedIdentifier,
            alternatives: Vec<OwnedIdentifier>,
        },
    }

    impl From<(u64, String)> for OwnedIdentifier {
        fn from(pair: (u64, String)) -> Self {
            OwnedIdentifier {
                id: pair.0,
                name: pair.1,
            }
        }
    }

    impl<T> From<(u64, String, T)> for OwnedIdentifier {
        fn from(triple: (u64, String, T)) -> Self {
            OwnedIdentifier {
                id: triple.0,
                name: triple.1,
            }
        }
    }

    /// A string which points to the path of a file or directory during the point
    /// of construction. A relative path is evaluated relative to the root
    /// 'kerchief.toml'.
    #[derive(Debug, PartialEq, Eq, Hash, Clone)]
    pub enum IncludePath {
        File(path::PathBuf),
        Dir(path::PathBuf),
    }

    #[derive(Debug, Error, PartialEq, Eq, Hash, Clone)]
    pub enum IncludeError {
        #[error("path {0} not found")]
        NotPresent(path::PathBuf),
    }

    impl IncludePath {
        fn try_from_root(root: &path::Path, path: &str) -> Result<Self, IncludeError> {
            let path = path::Path::new(path);
            let mut absolute = path::PathBuf::new();

            if path.is_relative() {
                absolute.push(root);
            }

            absolute.push(path);

            if absolute.is_file() {
                Ok(Self::File(path.to_owned()))
            } else if absolute.is_dir() {
                Ok(Self::Dir(path.to_owned()))
            } else {
                Err(IncludeError::NotPresent(absolute))
            }
        }

        pub fn path(&self) -> &path::Path {
            self.as_ref()
        }

        pub fn is_file(&self) -> bool {
            match self {
                Self::File(_) => true,
                _ => false,
            }
        }

        pub fn is_dir(&self) -> bool {
            match self {
                Self::Dir(_) => true,
                _ => false,
            }
        }
    }

    impl fmt::Display for IncludePath {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.path().to_string_lossy())
        }
    }

    impl AsRef<path::Path> for IncludePath {
        fn as_ref(&self) -> &path::Path {
            match self {
                Self::File(p) => p.as_ref(),
                Self::Dir(p) => p.as_ref(),
            }
        }
    }

    /// As the path is created from a string, we can unwrap it as a string.
    impl<'a> From<&'a IncludePath> for &'a str {
        fn from(p: &'a IncludePath) -> &'a str {
            p.path().to_str().unwrap()
        }
    }

    // impl fmt::Display for AssignmentValidation {
    //     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    //         match &self.ident {
    //             Err(e) => {
    //                 writeln!(f, "Assignment identifier error: {}", e)
    //             }
    //             Ok(ident) => {
    //                 writeln!(f, "'{}' (canvas id {})", ident.name, ident.id)?;
    //                 for (path, pathopts) in self.include.iter() {
    //                     write!(f, "  ")?;
    //                     match path {
    //                         Err(e) => {
    //                             writeln!(f, "Path error: {}", e)?;
    //                         }
    //                         Ok(path) => {
    //                             write!(f, "{}", path)?;
    //                             let mut opts: Vec<&FileOption> = Vec::new();
    //                             let mut errors: Vec<&FileOptionError> = Vec::new();
    //                             for opt in pathopts.iter() {
    //                                 match opt {
    //                                     Ok(opt) => opts.push(opt),
    //                                     Err(e) => errors.push(e),
    //                                 }
    //                             }

    //                             if opts.len() == 1 {
    //                                 write!(f, ", option: {}", opts[0])?;
    //                             } else if opts.len() > 1 {
    //                                 write!(f, ", options:")?;
    //                                 for opt in opts.iter() {
    //                                     write!(f, " {}", opt)?;
    //                                 }
    //                             }

    //                             if errors.len() > 0 {
    //                                 write!(f, ", unrecognized:")?;
    //                                 for err in errors.iter() {
    //                                     write!(f, " {}", err)?;
    //                                 }
    //                             }

    //                             writeln!(f, "")?;
    //                         }
    //                     }
    //                 }
    //                 Ok(())
    //             }
    //         }
    //     }
    // }

    #[derive(Debug, Error)]
    pub enum BuildError {
        #[error("parsing config failed: {0}")]
        Parse(#[from] toml::de::Error),
        #[error("course identifier error: {0}")]
        Ident(#[from] IdentifierError),
        #[error("course identifier error: {0}")]
        Iden(#[from] IdentifierErr),
        #[error("canvas api error: {0}")]
        Canvas(#[from] canvas::Error),
        #[error(transparent)]
        Fetch(#[from] cfg::FetchError),
    }

    // impl fmt::Display for Model {
    //     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    //         writeln!(
    //             f,
    //             "Course '{}' (canvas id {})",
    //             self.course.name, self.course.id
    //         )?;
    //         for (s, a) in self.assignments.iter() {
    //             writeln!(f, "")?;
    //             writeln!(f, "assignment.{}", s)?;
    //             a.fmt(f)?;
    //         }
    //         Ok(())
    //     }
    // }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Identifier<'a> {
        id: u64,
        name: &'a str,
    }

    #[derive(Clone, Debug, Error)]
    pub enum IdentifierErr {
        #[error("fields 'id', 'name' missing from config, either should be sufficient")]
        NotSpecified { alternatives: Vec<OwnedIdentifier> },
        #[error(
            "the name '{user_provided}' is not sufficient to distinguish among possible values"
        )]
        UnderSpecified {
            user_provided: String,
            alternatives: Vec<OwnedIdentifier>,
        },
        #[error(
            "the user provided identifier matches some canvas identifier by 'id' but not by 'name'"
        )]
        NameConflict {
            user_provided: OwnedIdentifier,
            match_id_not_name: Vec<OwnedIdentifier>,
        },
        #[error(
            "the user provided identifier matches some canvas identifier by 'name' but not by 'id'"
        )]
        IdConflict {
            user_provided: OwnedIdentifier,
            match_name_not_id: Vec<OwnedIdentifier>,
        },
        #[error("the id '{user_provided}' is not present among possible values")]
        NoSuchId {
            user_provided: u64,
            alternatives: Vec<OwnedIdentifier>,
        },
        #[error("the name '{user_provided}' is not present among possible values")]
        NoSuchName {
            user_provided: String,
            alternatives: Vec<OwnedIdentifier>,
        },
        #[error("the user provided identifier is not present among possible values")]
        NoSuchIdentifier {
            user_provided: OwnedIdentifier,
            alternatives: Vec<OwnedIdentifier>,
        },
    }

    impl<'a> Identifier<'a> {
        fn to_owned(self) -> OwnedIdentifier {
            let Identifier { id, name } = self;
            OwnedIdentifier {
                id,
                name: name.to_owned(),
            }
        }

        fn id(&self) -> u64 {
            self.id
        }

        fn name(&self) -> &'a str {
            self.name
        }

        fn try_match_among(
            matches: Vec<Self>,
            partial: &'a cfg::Identifier,
        ) -> Result<Self, IdentifierErr> {
            match &partial.read() {
                cfg::ReadIdentifier::NameAndId { name, id } => {
                    let mut id_matches: Vec<Self> = Vec::new();
                    let mut name_matches: Vec<Self> = Vec::new();
                    for ident in matches.clone().into_iter() {
                        if ident.id == *id {
                            id_matches.push(ident);
                        }
                        if ident.name == *name {
                            name_matches.push(ident);
                        }
                    }

                    if let Some(&id_match) = id_matches.get(0) {
                        if name_matches.contains(&id_match) {
                            Ok(id_match)
                        } else {
                            Err(IdentifierErr::NameConflict {
                                user_provided: Self {
                                    id: *id,
                                    name: *name,
                                }
                                .to_owned(),
                                match_id_not_name: id_matches
                                    .into_iter()
                                    .map(Self::to_owned)
                                    .collect(),
                            })
                        }

                    // no id matches
                    } else {
                        if name_matches.is_empty() {
                            Err(IdentifierErr::NoSuchIdentifier {
                                user_provided: Self {
                                    id: *id,
                                    name: *name,
                                }
                                .to_owned(),
                                alternatives: matches.into_iter().map(Self::to_owned).collect(),
                            })
                        } else {
                            Err(IdentifierErr::IdConflict {
                                user_provided: Self {
                                    id: *id,
                                    name: *name,
                                }
                                .to_owned(),
                                match_name_not_id: name_matches
                                    .into_iter()
                                    .map(Self::to_owned)
                                    .collect(),
                            })
                        }
                    }
                }

                cfg::ReadIdentifier::IdOnly { id } => {
                    let mut id_matches = matches.iter().filter(|ident| ident.id == *id);

                    if let Some(&id_match) = id_matches.next() {
                        Ok(id_match)
                    } else {
                        Err(IdentifierErr::NoSuchId {
                            user_provided: *id,
                            alternatives: matches.into_iter().map(Self::to_owned).collect(),
                        })
                    }
                }

                cfg::ReadIdentifier::NameOnly { name } => {
                    let name_matches: Vec<Self> = matches
                        .iter()
                        .filter(|ident| ident.name == *name)
                        .cloned()
                        .collect();
                    if let Some(&name_match) = name_matches.get(0) {
                        if name_matches.len() == 1 {
                            Ok(name_match.clone())
                        } else {
                            Err(IdentifierErr::UnderSpecified {
                                user_provided: (*name).to_owned(),
                                alternatives: name_matches
                                    .into_iter()
                                    .map(Self::to_owned)
                                    .collect(),
                            })
                        }
                    } else {
                        Err(IdentifierErr::NoSuchName {
                            user_provided: (*name).to_owned(),
                            alternatives: matches.into_iter().map(Self::to_owned).collect(),
                        })
                    }
                }
                cfg::ReadIdentifier::None => Err(IdentifierErr::NotSpecified {
                    alternatives: matches.into_iter().map(Self::to_owned).collect(),
                }),
            }
        }
    }

    impl<'a> From<&'a canvas::Course> for Identifier<'a> {
        fn from(course: &'a canvas::Course) -> Self {
            Self {
                name: course.name(),
                id: course.id(),
            }
        }
    }

    impl<'a> From<&'a canvas::Assignment> for Identifier<'a> {
        fn from(assignment: &'a canvas::Assignment) -> Self {
            Self {
                name: assignment.name(),
                id: assignment.id(),
            }
        }
    }

    pub struct Wall {
        user_cfg: cfg::Config,
        courses: OnceCell<Vec<canvas::Course>>,
        assignments: OnceCell<Vec<canvas::Assignment>>,
    }

    impl Wall {
        pub fn new(user_cfg: cfg::Config) -> Self {
            Self {
                user_cfg,
                courses: OnceCell::new(),
                assignments: OnceCell::new(),
            }
        }

        pub fn get_token(&self) -> &str {
            self.user_cfg.token()
        }

        pub fn get_domain(&self) -> &str {
            self.user_cfg.domain()
        }

        pub fn get_course_id(&self) -> Result<u64, BuildError> {
            let courses = self.get_courses()?.iter().map(Identifier::from).collect();
            let course_ident = Identifier::try_match_among(courses, self.user_cfg.course_ident())?;
            Ok(course_ident.id())
        }

        fn get_assignment_ident(&self, key: &str) -> Result<Identifier<'_>, BuildError> {
            let assignments = self
                .get_assignments()?
                .into_iter()
                .map(Identifier::from)
                .collect();
            let assignment_ident =
                Identifier::try_match_among(assignments, self.user_cfg.assignment(key)?.ident())?;

            Ok(assignment_ident)
        }

        pub fn get_assignment_id(&self, key: &str) -> Result<u64, BuildError> {
            Ok(self.get_assignment_ident(key)?.id())
        }

        pub fn get_assignment_file_paths<'a>(
            &'a self,
            key: &'a str,
            root: &'a path::Path,
        ) -> Result<
            impl Iterator<
                    Item = (
                        Result<IncludePath, IncludeError>,
                        Vec<Result<FileOption, FileOptionError>>,
                    ),
                > + 'a,
            BuildError,
        > {
            let include_paths = self.user_cfg.assignment(key)?.include();

            Ok(include_paths
                .into_iter()
                .map(|include| (include.path(), include.options()))
                .map(move |(p, opts)| {
                    (
                        IncludePath::try_from_root(root, p),
                        opts.into_iter().map(FileOption::try_from).collect(),
                    )
                }))
        }

        fn get_courses(&self) -> Result<&Vec<canvas::Course>, BuildError> {
            let courses = self
                .courses
                .get_or_try_init(|| canvas::get_courses(self.get_token(), self.get_domain()))?;

            Ok(courses)
        }

        fn get_selected_course(&self) -> Result<Identifier<'_>, BuildError> {
            let courses = self.get_courses()?.iter().map(Identifier::from).collect();
            let selected_course = self.user_cfg.course_ident();
            Ok(Identifier::try_match_among(courses, &selected_course)?)
        }

        fn get_assignments(&self) -> Result<&Vec<canvas::Assignment>, BuildError> {
            Ok(self
                .assignments
                .get_or_try_init(|| -> Result<_, BuildError> {
                    Ok(canvas::get_assignments(
                        self.get_token(),
                        self.get_domain(),
                        self.get_course_id()?,
                    )?)
                })?)
        }
    }
}

fn find_root() -> anyhow::Result<path::PathBuf> {
    Ok(path::Path::new(".").canonicalize()?)
}

use std::collections::HashSet;
use std::io::Write;
use std::io::{BufRead, Read};
use std::{env, fs, io, path};
use walkdir::WalkDir;

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
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp)?;

    for (p, opts) in store.get_assignment_file_paths(key, &root)? {
        if let Ok(include) = p {
            apply_include_transforms(&include, opts.into_iter().flatten().collect(), &temp)?;
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

use clap::{App, Arg, SubCommand};
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

        let mut cfg_file = String::new();
        fs::File::open("kerchief.toml")?.read_to_string(&mut cfg_file)?;
        let cfg: cfg::Config = toml::from_str(&cfg_file)?;
        let store: model::Wall = model::Wall::new(cfg);

        let upload_dir = stage_includes(&store, key)?;
        println!(
            "Preparing to upload the following items (located in {})",
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
                break;
            } else if line.starts_with(&['n', 'N'][..]) {
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
