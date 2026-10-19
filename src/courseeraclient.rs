use std::{fs, sync::RwLock};

use reqwest::header::{self, COOKIE, HeaderValue, USER_AGENT};

use serde_json::json;

use crate::courseeraresponses::{
    course_info, course_materials, course_membership_status, enroll_in_course, lecture_videos,
    specialization_info, supplements, user_permission_info,
};

pub const COURSERA_BASE: &str = "https://www.coursera.org";
const USER_AGENT_STR: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

pub struct CourseEraClient {
    client: RwLock<reqwest::Client>,
    cookie: RwLock<String>,
    persist_cookie_file: bool,
    pub user_id: RwLock<String>,
    base_url: String,
    asset_client: reqwest::Client,
}

impl CourseEraClient {
    pub fn new() -> Self {
        let persist_cookie_file = !cfg!(test);
        let initial_cookie = if persist_cookie_file {
            get_cookie_from_file()
        } else {
            String::new()
        };
        Self {
            client: RwLock::new(build_reqwest_client(&initial_cookie)),
            cookie: RwLock::new(initial_cookie),
            persist_cookie_file,
            user_id: RwLock::new(String::new()),
            base_url: COURSERA_BASE.to_string(),
            asset_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("asset HTTP client"),
        }
    }

    fn http(&self) -> reqwest::Client {
        self.client.read().unwrap().clone()
    }

    pub async fn api_json(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<serde_json::Value, String> {
        let response = self
            .http()
            .get(format!("{}{path}", self.base_url))
            .query(query)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
            .map_err(|_| "Could not reach Coursera. Try again.".to_string())?;
        match response.status().as_u16() {
            401 | 403 => {
                return Err(
                    "Coursera access denied. Check cookie.txt and your course access.".into(),
                );
            }
            404 => return Err("Coursera could not find that course or specialisation.".into()),
            429 => return Err("Coursera rate limit reached. Try again later.".into()),
            200..=299 => {}
            status => return Err(format!("Coursera returned HTTP {status}. Try again later.")),
        }
        response
            .json()
            .await
            .map_err(|_| "Coursera returned an invalid response. Check cookie.txt.".into())
    }

    pub async fn asset_response(&self, source: &str) -> Result<reqwest::Response, String> {
        let url = reqwest::Url::parse(&self.base_url)
            .map_err(|_| "Invalid provider URL")?
            .join(source)
            .map_err(|_| "Invalid asset URL")?;
        if !["https", "http"].contains(&url.scheme()) {
            return Err("Unsupported asset URL".into());
        }
        // Only send the Coursera session cookie to Coursera itself.
        let provider = reqwest::Url::parse(&self.base_url).map_err(|_| "Invalid provider URL")?;
        let client = if url.origin() == provider.origin() {
            self.http()
        } else {
            self.asset_client.clone()
        };
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|_| "Could not fetch the course file".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Course file returned HTTP {}",
                response.status().as_u16()
            ));
        }
        Ok(response)
    }

    #[cfg(test)]
    pub fn for_test(base_url: String) -> Self {
        Self {
            client: RwLock::new(reqwest::Client::new()),
            cookie: RwLock::new(String::new()),
            persist_cookie_file: false,
            user_id: RwLock::new(String::new()),
            base_url,
            asset_client: reqwest::Client::new(),
        }
    }

    pub fn get_cookie(&self) -> String {
        if self.persist_cookie_file {
            let from_disk = get_cookie_from_file();
            *self.cookie.write().unwrap() = from_disk.clone();
            from_disk
        } else {
            self.cookie.read().unwrap().clone()
        }
    }

    pub fn update_cookie(&self, cookie: &str) -> Result<(), String> {
        if self.persist_cookie_file {
            set_cookie_to_file(cookie)?;
        }
        *self.cookie.write().unwrap() = cookie.to_string();
        *self.client.write().unwrap() = build_reqwest_client(cookie);
        *self.user_id.write().unwrap() = String::new();
        Ok(())
    }

    pub async fn set_cookie(&self, cookie: &str) {
        let _ = self.update_cookie(cookie);
        self.set_user_id().await;
    }

    pub async fn set_user_id(&self) {
        if let Ok(data) = self.get_user_id_info().await {
            if let Some(element) = data.elements.first() {
                *self.user_id.write().unwrap() = element.id.clone();
            }
        }
    }

    pub async fn get_user_id(&self) -> reqwest::Result<String> {
        if self.user_id.read().unwrap().is_empty() {
            self.set_user_id().await;
        }
        Ok(self.user_id.read().unwrap().clone())
    }

    pub async fn get_specialization_info(
        &self,
        spec_slug: &str,
    ) -> reqwest::Result<specialization_info::Response> {
        let url = format!(
            "{COURSERA_BASE}/api/onDemandSpecializations.v1\
                   ?q=slug\
                   &slug={spec_slug}\
                   &includes=courseIds,partnerIds\
                   &fields=courseIds,name,slug,description,tagline,logo,photoUrl,promoPhoto,partnerIds,\
                   partners.v1(name,shortName,squareLogo),\
                   courses.v1(name,slug,description,photoUrl,promoPhoto)"
        );

        let response = self.http().get(&url).send().await?;
        let data: specialization_info::Response = response.json().await?;
        Ok(data)
    }

    pub async fn get_course_info(
        &self,
        course_slug: &str,
    ) -> reqwest::Result<course_info::Response> {
        let url = format!(
            "{COURSERA_BASE}/api/onDemandCourses.v1\
                   ?q=slug\
                   &slug={course_slug}\
                   &includes=partnerIds\
                   &fields=name,slug,description,photoUrl,promoPhoto,partnerIds,\
                   partners.v1(name,shortName,squareLogo)"
        );
        let response = self.http().get(&url).send().await?;
        let data: course_info::Response = response.json().await?;
        Ok(data)
    }

    pub async fn fetch_user_id(&self) -> Option<i64> {
        let existing = self.user_id.read().unwrap().clone();
        if let Ok(id) = existing.parse::<i64>() {
            if id > 0 {
                return Some(id);
            }
        }
        for attempt in 0..2 {
            if attempt == 1 && !self.base_url.starts_with("http://127.0.0.1") {
                let raw = self.get_cookie();
                *self.client.write().unwrap() = build_reqwest_client(&raw);
            }
            let Ok(response) = self
                .http()
                .get(format!(
                    "{}/api/adminUserPermissions.v1?q=my",
                    self.base_url
                ))
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await
            else {
                continue;
            };
            let Ok(data) = response.json::<serde_json::Value>().await else {
                continue;
            };
            let Some(raw_id) = data.pointer("/elements/0/id") else {
                continue;
            };
            let Some(parsed) = raw_id
                .as_str()
                .and_then(|s| s.parse::<i64>().ok())
                .or_else(|| raw_id.as_i64())
            else {
                continue;
            };
            if parsed > 0 {
                *self.user_id.write().unwrap() = parsed.to_string();
                return Some(parsed);
            }
        }
        None
    }

    pub async fn ensure_specialization_enrolled(&self, spec_id: &str) {
        let Some(user_id) = self.fetch_user_id().await else {
            return;
        };
        let payload = json!({
            "s12nId": spec_id,
            "userId": user_id,
            "role": "LEARNER"
        });
        let _ = self
            .http()
            .post(format!(
                "{}/api/onDemandSpecializationMemberships.v1",
                self.base_url
            ))
            .json(&payload)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;
    }

    pub async fn ensure_course_enrolled(&self, course_id: &str, _course_slug: &str) {
        self.force_enroll_course(course_id).await;
    }

    pub async fn force_enroll_course(&self, course_id: &str) {
        let Some(user_id) = self.fetch_user_id().await else {
            return;
        };
        let payload = json!({
            "courseId": course_id,
            "userId": user_id,
            "courseRole": "LEARNER"
        });
        let _ = self
            .http()
            .post(format!("{}/api/openCourseMemberships.v1", self.base_url))
            .json(&payload)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;
    }

    async fn get_user_id_info(&self) -> reqwest::Result<user_permission_info::Response> {
        let url = format!("{}/api/adminUserPermissions.v1?q=my", self.base_url);
        let response = self.http().get(&url).send().await?;
        let data: user_permission_info::Response = response.json().await?;
        Ok(data)
    }

    pub async fn get_course_membership_status(
        &self,
        course_slug: &str,
    ) -> reqwest::Result<course_membership_status::Response> {
        let user_id = self.get_user_id().await?;
        let url = format!(
            "{}/api/openCourseMemberships.v1\
                   ?q=findByUserAndSlug\
                   &slug={course_slug}\
                   &userId={user_id}\
                   &fields=courseRole",
            self.base_url
        );
        let response = self.http().get(&url).send().await?;
        let data: course_membership_status::Response = response.json().await?;
        Ok(data)
    }

    pub async fn enroll_in_course(
        &self,
        course_id: &str,
    ) -> reqwest::Result<enroll_in_course::Response> {
        let url = format!("{}/api/openCourseMemberships.v1", self.base_url);
        let user_id = self
            .get_user_id()
            .await
            .unwrap_or("0".to_string())
            .parse::<i64>()
            .unwrap_or(0);
        let payload = json!({
            "courseId": course_id,
            "userId": user_id,
            "courseRole": "LEARNER"
        });
        let response = self.http().post(&url).json(&payload).send().await?;
        let data: enroll_in_course::Response = response.json().await?;
        Ok(data)
    }

    pub async fn enroll_in_specialization(
        &self,
        spec_id: &str,
    ) -> reqwest::Result<reqwest::Response> {
        let url = format!("{}/api/onDemandSpecializationMemberships.v1", self.base_url);
        let user_id = self
            .get_user_id()
            .await
            .unwrap_or("0".to_string())
            .parse::<i64>()
            .unwrap_or(0);

        let payload = json!({
            "s12nId": spec_id,
            "userId": user_id,
            "role": "LEARNER"
        });
        self.http().post(&url).json(&payload).send().await
    }

    pub async fn get_course_materials(
        &self,
        course_slug: &str,
    ) -> reqwest::Result<course_materials::Response> {
        let url = format!(
            "{COURSERA_BASE}/api/onDemandCourseMaterials.v2/\
                   ?q=slug\
                   &slug={course_slug}\
                   &includes=modules,lessons,items\
                   &fields=moduleIds,\
                   onDemandCourseMaterialModules.v1(name,slug,description,timeCommitment,lessonIds,optional,learningObjectives),\
                   onDemandCourseMaterialLessons.v1(name,slug,timeCommitment,elementIds,itemIds,optional),\
                   onDemandCourseMaterialItems.v2(name,slug,description,timeCommitment,contentSummary,isLocked,trackId)\
                   &showLockedItems=true"
        );
        let response = self.http().get(&url).send().await?;
        let data: course_materials::Response = response.json().await?;
        Ok(data)
    }

    pub async fn get_lecture_video(
        &self,
        course_id: &str,
        item_id: &str,
    ) -> reqwest::Result<lecture_videos::Response> {
        let url = format!(
            "{COURSERA_BASE}/api/onDemandLectureVideos.v1/{course_id}~{item_id}\
                   ?includes=video\
                   &fields=onDemandVideos.v1(sources,subtitles,subtitlesVtt,subtitlesTxt)"
        );
        let response = self.http().get(&url).send().await?;
        let data: lecture_videos::Response = response.json().await?;
        Ok(data)
    }

    pub async fn get_supplement(
        &self,
        course_id: &str,
        item_id: &str,
    ) -> reqwest::Result<supplements::Response> {
        let url = format!(
            "{COURSERA_BASE}/api/onDemandSupplements.v1/{course_id}~{item_id}\
                   ?includes=asset\
                   &fields=openCourseAssets.v1(typeName,definition)"
        );
        let response = self.http().get(&url).send().await?;
        let data: supplements::Response = response.json().await?;
        Ok(data)
    }

    pub async fn get_subtitle_or_asset(
        &self,
        path_or_url: &str,
    ) -> reqwest::Result<reqwest::Response> {
        let url = if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
            path_or_url.to_string()
        } else {
            format!("{COURSERA_BASE}{path_or_url}")
        };
        self.http().get(&url).send().await
    }
}

fn build_reqwest_client(raw_cookie: &str) -> reqwest::Client {
    let (cookie, csrf_token) = parse_cookie_header(raw_cookie);

    let mut headers = header::HeaderMap::new();

    if !cookie.is_empty() {
        if let Ok(cookie_value) = HeaderValue::from_str(&cookie) {
            headers.insert(COOKIE, cookie_value);
        }
    }

    if let Some(csrf_token) = csrf_token {
        if let Ok(csrf_token_value) = HeaderValue::from_str(&csrf_token) {
            headers.insert("x-csrf-token", csrf_token_value.clone());
            headers.insert("x-csrf3-token", csrf_token_value);
        }
    }
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_STR));
    headers.insert(
        "x-coursera-application",
        HeaderValue::from_static("ondemand"),
    );

    headers.insert(
        "x-requested-with",
        HeaderValue::from_static("XMLHttpRequest"),
    );

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap()
}

fn cookie_file_path() -> std::path::PathBuf {
    let data_cookie = crate::cookie_path();
    for candidate in [
        data_cookie.clone(),
        std::path::PathBuf::from("cookie.txt"),
        std::path::PathBuf::from("../data/cookie.txt"),
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/data/cookie.txt")),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    data_cookie
}

fn get_cookie_from_file() -> String {
    fs::read_to_string(cookie_file_path()).unwrap_or_default()
}

fn set_cookie_to_file(cookie: &str) -> Result<(), String> {
    let path = cookie_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create cookie directory: {e}"))?;
    }
    fs::write(&path, cookie).map_err(|e| format!("Could not write {}: {e}", path.display()))
}

fn parse_cookie_header(raw: &str) -> (String, Option<String>) {
    let raw_trimmed = raw.trim();
    let mut pairs = Vec::new();
    let mut csrf_token = None;

    // 1. Try parsing JSON format (e.g. Cookie-Editor JSON export [ { "name": "CAUTH", "value": "..." } ])
    if raw_trimmed.starts_with('[') && raw_trimmed.ends_with(']') {
        if let Ok(json_arr) = serde_json::from_str::<Vec<serde_json::Value>>(raw_trimmed) {
            for item in json_arr {
                if let (Some(name), Some(val)) = (
                    item.get("name").and_then(|v| v.as_str()),
                    item.get("value").and_then(|v| v.as_str()),
                ) {
                    if name == "CSRF3-Token" {
                        csrf_token = Some(val.to_string());
                    }
                    pairs.push(format!("{}={}", name, val));
                }
            }
            if !pairs.is_empty() {
                return (pairs.join("; "), csrf_token);
            }
        }
    }

    // 2. Try semicolon-separated or tab-separated / Netscape / newline-separated format
    for raw_line in raw_trimmed.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        // Handle Cookie-Editor / Netscape `#HttpOnly_` prefix
        if line.starts_with("#HttpOnly_") {
            line = &line[10..];
        } else if line.starts_with('#') {
            continue;
        }

        // Tab-separated (Netscape format)
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 7 {
            let name = parts[5].trim();
            let value = parts[6].trim();
            if name == "CSRF3-Token" {
                csrf_token = Some(value.to_string());
            }
            pairs.push(format!("{}={}", name, value));
            continue;
        }

        // Semicolon-separated pairs in a single line (document.cookie or raw Cookie header format)
        for part in line.split(';') {
            let part_trimmed = part.trim();
            if part_trimmed.is_empty() {
                continue;
            }
            if let Some((k, v)) = part_trimmed.split_once('=') {
                let key = k.trim();
                let val = v.trim();
                if key == "CSRF3-Token" {
                    csrf_token = Some(val.to_string());
                }
                pairs.push(format!("{}={}", key, val));
            }
        }
    }

    let cookie_str = if pairs.is_empty() {
        raw_trimmed.to_string()
    } else {
        pairs.join("; ")
    };

    (cookie_str, csrf_token)
}
