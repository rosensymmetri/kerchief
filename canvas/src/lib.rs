use chrono::prelude::*;
use reqwest::blocking::multipart::Form;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use url::Url;

#[derive(Deserialize, Debug)]
pub struct FileUploadEntry {
    upload_url: String,
    upload_params: HashMap<String, String>,
}

#[derive(Deserialize, Debug)]
pub struct FileUploadResponse {
    id: u64,
}

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
    due_at: DateTime<Local>,
    lock_at: Option<DateTime<Local>>,
    unlock_at: Option<DateTime<Local>>,
    submission: Option<Submission>,
}

impl Assignment {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn due_at(&self) -> &DateTime<Local> {
        &self.due_at
    }

    pub fn lock_at(&self) -> Option<&DateTime<Local>> {
        (&self.lock_at).as_ref()
    }

    pub fn unlock_at(&self) -> Option<&DateTime<Local>> {
        (&self.unlock_at).as_ref()
    }
}

#[derive(Clone, Deserialize, Debug)]
pub struct Submission {
    #[serde(flatten)]
    submission_type: SubmissionTypeResponse,
    submitted_at: DateTime<Local>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "submission_type")]
pub enum SubmissionTypeResponse {
    OnlineTextEntry { body: String },
    OnlineUrl { url: Url },
    // Can an online upload have 0 attached files? Hmm
    OnlineUpload { attachments: Vec<Attachment> },
    MediaRecording,
}

impl Submission {
    pub fn submitted_at(&self) -> &DateTime<Local> {
        &self.submitted_at
    }

    pub fn submission_type_response(&self) -> &SubmissionTypeResponse {
        &self.submission_type
    }
}

#[derive(Clone, Deserialize, Debug)]
struct SubmissionResponse {
    #[serde(flatten)]
    inner: Option<Submission>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Attachment {
    display_name: String,
    filename: String,
}

impl Attachment {
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn filename(&self) -> &str {
        &self.filename
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

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GetSubmissionInclude {
    SubmissionHistory,
    SubmissionComments,
    RubricAssessment,
    Assignment,
    Visibility,
    Course,
    User,
    Group,
}

pub fn get_assignments(token: &str, domain: &str, course_id: u64) -> Result<Vec<Assignment>> {
    let response: Vec<Assignment> = Client::new()
        .get(&format!(
            "https://{}/api/v1/courses/{}/assignments",
            domain, course_id
        ))
        .bearer_auth(token)
        .send()?
        .error_for_status()?
        .json()?;
    Ok(response)
}

pub fn get_single_submission(
    token: &str,
    domain: &str,
    course_id: u64,
    assignment_id: u64,
) -> Result<Option<Submission>> {
    Ok(Client::new()
        .get(&format!(
            "https://{}/api/v1/courses/{}/assignments/{}/submissions/self",
            domain, course_id, assignment_id
        ))
        .query(&[("include[]", "submission_comments")])
        .bearer_auth(token)
        .send()?
        .error_for_status()?
        .json::<SubmissionResponse>()?
        .inner)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
