use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::Read;
use std::sync::{RwLock, RwLockReadGuard};
use std::{fmt, fs, io, path};
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
    fn try_find(path: &str) -> Result<Self, IncludeError> {
        let path = path::Path::new(path);

        if path.is_file() {
            Ok(Self::File(path.to_owned()))
        } else if path.is_dir() {
            Ok(Self::Dir(path.to_owned()))
        } else {
            Err(IncludeError::NotPresent(path.to_owned()))
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

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("parsing config failed: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("course identifier error: {0}")]
    Identity(#[from] IdentifyErr),
    #[error("canvas api error: {0}")]
    Canvas(#[from] canvas::Error),
    #[error(transparent)]
    Fetch(#[from] config::FetchError),
}

pub trait Identify {
    fn name(&self) -> &str;
    fn id(&self) -> u64;
}

impl Identify for canvas::Assignment {
    fn name(&self) -> &str {
        self.name()
    }

    fn id(&self) -> u64 {
        self.id()
    }
}

impl Identify for canvas::Course {
    fn name(&self) -> &str {
        self.name()
    }

    fn id(&self) -> u64 {
        self.id()
    }
}

#[derive(Clone, Debug, Error)]
pub enum IdentifyErr {
    #[error("both 'id' and 'name' missing")]
    NotSpecified,
    #[error("the name '{0}' is not sufficient to distinguish among possible values")]
    UnderSpecified(String),
    #[error("the id '{1}' matches but not by name '{0}'")]
    NameConflict(String, u64),
    #[error("the name '{0}' matches but not by id '{1}'")]
    IdConflict(String, u64),
    #[error("the id '{0}' is not present among possible values")]
    NoSuchId(u64),
    #[error("the name '{0}' is not present among possible values")]
    NoSuchName(String),
    #[error("the user provided identifier is not present among possible values")]
    NeitherNameNorId(String, u64),
}

impl IdentifyErr {
    fn not_specified() -> Self {
        Self::NotSpecified
    }

    fn under_specified(name: &str) -> Self {
        Self::UnderSpecified(name.to_owned())
    }

    fn name_conflict(name: &str, id: u64) -> Self {
        Self::NameConflict(name.to_owned(), id)
    }

    fn id_conflict(name: &str, id: u64) -> Self {
        Self::IdConflict(name.to_owned(), id)
    }

    fn no_such_id(id: u64) -> Self {
        Self::NoSuchId(id)
    }

    fn no_such_name(name: &str) -> Self {
        Self::NoSuchName(name.to_owned())
    }

    fn neither_name_nor_id(name: &str, id: u64) -> Self {
        Self::NeitherNameNorId(name.to_owned(), id)
    }
}

pub fn try_match_among<'read, I: Identify>(
    matches: impl Iterator<Item = &'read I>,
    partial_name: Option<&str>,
    partial_id: Option<u64>,
) -> Result<&'read I, IdentifyErr> {
    match (partial_name, partial_id) {
        (Some(name), Some(id)) => {
            let mut id_matches: Vec<&I> = Vec::new();
            let mut name_matches: Vec<&I> = Vec::new();
            for i in matches {
                if i.id() == id {
                    id_matches.push(i);
                }
                if i.name() == name {
                    name_matches.push(i);
                }
            }

            if let Some(id_match) = id_matches.get(0) {
                if name_matches.iter().any(|i| id_match.name() == i.name()) {
                    Ok(id_match)
                } else {
                    Err(IdentifyErr::name_conflict(name, id))
                }

            // no id matches
            } else {
                if name_matches.is_empty() {
                    Err(IdentifyErr::neither_name_nor_id(name, id))
                } else {
                    Err(IdentifyErr::id_conflict(name, id))
                }
            }
        }

        (None, Some(id)) => {
            let mut id_matches = matches.filter(|i| i.id() == id);
            if let Some(id_match) = id_matches.next() {
                Ok(id_match)
            } else {
                Err(IdentifyErr::no_such_id(id))
            }
        }

        (Some(name), None) => {
            let mut name_matches = matches.filter(|i| i.name() == name);
            if let Some(name_match) = name_matches.next() {
                if name_matches.next().is_none() {
                    Ok(name_match)
                } else {
                    Err(IdentifyErr::under_specified(name))
                }
            } else {
                Err(IdentifyErr::no_such_name(name))
            }
        }
        (None, None) => Err(IdentifyErr::not_specified()),
    }
}

type SubmissionGuard = RwLock<Option<canvas::Submission>>;

pub struct Wall {
    user_cfg: config::Config,
    courses: OnceCell<Vec<canvas::Course>>,
    assignments: OnceCell<Vec<canvas::Assignment>>,
    // The keys are corresponding assignment id's.
    latest_submissions: OnceCell<HashMap<u64, OnceCell<SubmissionGuard>>>,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
    #[error(transparent)]
    Read(#[from] io::Error),
}

impl Wall {
    pub fn try_from_path<P: AsRef<path::Path>>(p: P) -> Result<Self, ParseError> {
        let mut buf = String::new();
        fs::File::open(p)?.read_to_string(&mut buf)?;
        Ok(Self::new(toml::from_str(&buf)?))
    }

    pub fn new(user_cfg: config::Config) -> Self {
        Self {
            user_cfg,
            courses: OnceCell::new(),
            assignments: OnceCell::new(),
            latest_submissions: OnceCell::new(),
        }
    }

    pub fn get_token(&self) -> &str {
        self.user_cfg.token()
    }

    pub fn get_domain(&self) -> &str {
        self.user_cfg.domain()
    }

    pub fn get_course_id(&self) -> Result<u64, BuildError> {
        Ok(self.get_selected_course()?.id())
    }

    pub fn get_assignment(&self, key: &str) -> Result<&canvas::Assignment, BuildError> {
        Ok(try_match_among(
            self.get_assignments()?.iter(),
            self.user_cfg.assignment(key)?.get_name(),
            self.user_cfg.assignment(key)?.get_id(),
        )?)
    }

    fn get_latest_submissions(
        &self,
    ) -> Result<&HashMap<u64, OnceCell<SubmissionGuard>>, BuildError> {
        Ok(self.latest_submissions.get_or_try_init(
            || -> Result<HashMap<u64, OnceCell<SubmissionGuard>>, BuildError> {
                let mut map = HashMap::new();
                for a in self.get_assignments()?.iter() {
                    map.insert(a.id(), OnceCell::new());
                }
                Ok(map)
            },
        )?)
    }

    pub fn get_latest_submission(
        &self,
        assignment_id: u64,
    ) -> Result<RwLockReadGuard<'_, Option<canvas::Submission>>, BuildError> {
        Ok(self
            .get_latest_submissions()?
            .get(&assignment_id)
            .expect("The key should be an assignment id for which the map should have value.")
            .get_or_try_init(|| -> Result<SubmissionGuard, BuildError> {
                Ok(RwLock::new(canvas::get_single_submission(
                    self.get_token(),
                    self.get_domain(),
                    self.get_course_id()?,
                    assignment_id,
                )?))
            })?
            .read()
            .expect("Encountered poisoned lock."))
    }

    pub fn get_assignment_file_paths<'a>(
        &'a self,
        key: &'a str,
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
                    IncludePath::try_find(p),
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

    pub fn get_selected_course(&self) -> Result<&canvas::Course, BuildError> {
        let courses = self.get_courses()?.iter();
        let name = self.user_cfg.get_course_name();
        let id = self.user_cfg.get_course_id();
        Ok(try_match_among(courses, name, id)?)
    }

    pub fn get_assignments(&self) -> Result<&Vec<canvas::Assignment>, BuildError> {
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

    pub fn iter_assignments(
        &self,
    ) -> Result<impl Iterator<Item = &canvas::Assignment> + '_, BuildError> {
        Ok(self.get_assignments()?.iter())
    }
}

#[cfg(test)]
mod tests {}
