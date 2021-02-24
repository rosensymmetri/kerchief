use once_cell::unsync::OnceCell;
use std::convert::TryFrom;
use std::io::Read;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedIdentifier {
    id: u64,
    name: String,
}

impl OwnedIdentifier {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for Identifier<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (id {})", self.id(), self.name())
    }
}

impl fmt::Display for OwnedIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (id {})", self.id(), self.name())
    }
}

// impl fmt::Display for Vec<OwnedIdentifier> {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "[")?;
//         for ident in self.iter().take(self.len()) {
//             write!(f, "{}, ", ident)?;
//         }
//         for ident in self.iter().last() {
//             write!(f, "{}", ident)?;
//         }
//         write!(f, "]")
//     }
// }

#[derive(Clone, Debug, Error)]
pub enum IdentifierError {
    #[error("fields 'id', 'name' missing from config, either should be sufficient")]
    NotSpecified { alternatives: Vec<OwnedIdentifier> },
    #[error("the name '{user_provided}' is not sufficient to distinguish among possible values")]
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
    Ident(#[from] IdentifierError),
    #[error("course identifier error: {0}")]
    Iden(#[from] IdentifierErr),
    #[error("canvas api error: {0}")]
    Canvas(#[from] canvas::Error),
    #[error(transparent)]
    Fetch(#[from] config::FetchError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identifier<'a> {
    id: u64,
    name: &'a str,
}

#[derive(Clone, Debug, Error)]
pub enum IdentifierErr {
    #[error("fields 'id', 'name' missing from config, either should be sufficient")]
    NotSpecified { alternatives: Vec<OwnedIdentifier> },
    #[error("the name '{user_provided}' is not sufficient to distinguish among possible values")]
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
        partial: &'a config::Identifier,
    ) -> Result<Self, IdentifierErr> {
        match &partial.read() {
            config::ReadIdentifier::NameAndId { name, id } => {
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
                            match_id_not_name: id_matches.into_iter().map(Self::to_owned).collect(),
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

            config::ReadIdentifier::IdOnly { id } => {
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

            config::ReadIdentifier::NameOnly { name } => {
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
                            alternatives: name_matches.into_iter().map(Self::to_owned).collect(),
                        })
                    }
                } else {
                    Err(IdentifierErr::NoSuchName {
                        user_provided: (*name).to_owned(),
                        alternatives: matches.into_iter().map(Self::to_owned).collect(),
                    })
                }
            }
            config::ReadIdentifier::None => Err(IdentifierErr::NotSpecified {
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
    user_cfg: config::Config,
    courses: OnceCell<Vec<canvas::Course>>,
    assignments: OnceCell<Vec<canvas::Assignment>>,
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

    pub fn get_assignment_name(&self, key: &str) -> Result<&str, BuildError> {
        Ok(self.get_assignment_ident(key)?.name())
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

#[cfg(test)]
mod tests {}
