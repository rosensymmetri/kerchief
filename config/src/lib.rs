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
    name: Option<String>,
    id: Option<u64>,
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
enum Include {
    Single(Path),
    Many(Vec<Path>),
}

#[cfg(test)]
mod tests {
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
