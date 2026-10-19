pub mod specialization_info {
    use serde::{Deserialize, Serialize};

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Response {
        pub elements: Vec<Element>,
        pub paging: Paging,
        pub linked: Linked,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Element {
        pub course_ids: Vec<String>,
        pub description: String,
        pub partner_ids: Vec<String>,
        pub name: String,
        pub tagline: String,
        pub logo: String,
        pub id: String,
        pub slug: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Paging {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Linked {
        #[serde(rename = "courses.v1")]
        pub courses_v1: Vec<CoursesV1>,
        #[serde(rename = "partners.v1")]
        pub partners_v1: Vec<PartnersV1>,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CoursesV1 {
        pub course_type: String,
        pub description: String,
        pub name: String,
        pub id: String,
        pub slug: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PartnersV1 {
        pub name: String,
        pub id: String,
        pub square_logo: String,
        pub short_name: String,
    }
}

pub mod course_info {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Response {
        pub elements: Vec<Element>,
        pub paging: Paging,
        pub linked: Linked,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Element {
        pub is_real: bool,
        pub estimated_workload: String,
        pub description: String,
        pub is_restricted_membership: bool,
        pub verification_enabled_at: i64,
        pub is_verification_enabled: bool,
        pub preview_user_ids: Vec<Value>,
        pub id: String,
        pub slug: String,
        pub instructor_ids: Vec<String>,
        pub subtitle_language_codes: Vec<String>,
        pub enrollable_site_user_roles: Vec<Value>,
        pub promo_photo: String,
        pub primary_language_codes: Vec<String>,
        pub partner_ids: Vec<String>,
        pub category_ids: Vec<Value>,
        pub is_subtitle_translation_enabled: bool,
        pub name: String,
        pub course_status: String,
        pub launched_at: i64,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Paging {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Linked {
        #[serde(rename = "partners.v1")]
        pub partners_v1: Vec<PartnersV1>,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PartnersV1 {
        pub class_logo: String,
        pub name: String,
        #[serde(default)]
        pub home_link: String,
        pub logo: String,
        pub id: String,
        pub square_logo: String,
        pub short_name: String,
    }
}

pub mod user_permission_info {
    use serde::{Deserialize, Serialize};

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Response {
        pub elements: Vec<Element>,
        pub paging: Paging,
        pub linked: Linked,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Element {
        pub permissions: Permissions,
        pub id: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Permissions {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Paging {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Linked {}
}

pub mod course_membership_status {
    use serde::{Deserialize, Serialize};

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Response {
        pub elements: Vec<Element>,
        pub paging: Paging,
        pub linked: Linked,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Element {
        pub course_role: String,
        pub id: String,
        pub user_id: i64,
        pub course_id: String,
        pub timestamp: i64,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Paging {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Linked {}
}

pub mod course_materials {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Response {
        pub elements: Vec<Element>,
        pub paging: Paging,
        pub linked: Linked,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Element {
        pub module_ids: Vec<String>,
        pub id: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Paging {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Linked {
        #[serde(rename = "onDemandCourseMaterialLessons.v1")]
        pub on_demand_course_material_lessons_v1: Vec<OnDemandCourseMaterialLessonsV1>,
        #[serde(rename = "onDemandCourseMaterialItems.v2")]
        pub on_demand_course_material_items_v2: Vec<OnDemandCourseMaterialItemsV2>,
        #[serde(rename = "onDemandCourseMaterialModules.v1")]
        pub on_demand_course_material_modules_v1: Vec<OnDemandCourseMaterialModulesV1>,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OnDemandCourseMaterialLessonsV1 {
        pub item_ids: Vec<String>,
        pub name: String,
        pub time_commitment: i64,
        pub id: String,
        pub module_id: String,
        pub slug: String,
        pub element_ids: Vec<String>,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OnDemandCourseMaterialItemsV2 {
        pub track_id: String,
        pub is_locked: bool,
        pub name: String,
        pub lesson_id: String,
        pub time_commitment: i64,
        pub id: String,
        pub module_id: String,
        pub content_summary: ContentSummary,
        pub slug: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ContentSummary {
        pub type_name: String,
        pub definition: Definition,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Definition {
        pub asset_type_name: Option<String>,
        pub contains_widget: Option<bool>,
        pub duration: Option<i64>,
        pub has_in_video_assessment: Option<bool>,
        pub assignment_features: Option<Vec<String>>,
        pub standard_proctor_configuration_id: Option<String>,
        pub is_team_submission_enabled: Option<bool>,
        #[serde(default)]
        pub required_mobile_features: Vec<Value>,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OnDemandCourseMaterialModulesV1 {
        pub learning_objectives: Vec<String>,
        pub name: String,
        pub description: String,
        pub time_commitment: i64,
        pub id: String,
        pub lesson_ids: Vec<String>,
        pub slug: String,
    }
}

pub mod lecture_videos {
    use serde::{Deserialize, Serialize};

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Response {
        pub elements: Vec<Element>,
        pub paging: Paging,
        pub linked: Linked,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Element {
        pub item_id: String,
        pub id: String,
        pub course_id: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Paging {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Linked {
        #[serde(rename = "onDemandVideos.v1")]
        pub on_demand_videos_v1: Vec<OnDemandVideosV1>,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OnDemandVideosV1 {
        pub subtitles: Subtitles,
        pub subtitles_txt: SubtitlesTxt,
        pub sources: Sources,
        pub subtitles_vtt: SubtitlesVtt,
        pub id: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Subtitles {
        pub ar: String,
        pub ko: String,
        pub ja: String,
        pub en: String,
        pub it: String,
        pub id: String,
        pub fr: String,
        pub es: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SubtitlesTxt {
        pub ar: String,
        pub ko: String,
        pub ja: String,
        pub en: String,
        pub it: String,
        pub id: String,
        pub fr: String,
        pub es: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Sources {
        pub playlists: Playlists,
        pub audio: Vec<Audio>,
        pub by_resolution: ByResolution,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Playlists {
        #[serde(rename = "mpeg-dash")]
        pub mpeg_dash: String,
        pub hls: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Audio {
        pub audio_format: String,
        pub bitrate: i64,
        pub url: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ByResolution {
        #[serde(rename = "1080p")]
        pub n1080p: n1080p,
        #[serde(rename = "720p")]
        pub n720p: n720p,
        #[serde(rename = "360p")]
        pub n360p: n360p,
        #[serde(rename = "240p")]
        pub n240p: n240p,
        #[serde(rename = "540p")]
        pub n540p: n540p,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct n1080p {
        #[serde(rename = "webMVideoUrl")]
        pub web_mvideo_url: String,
        #[serde(rename = "mp4VideoUrl")]
        pub mp4video_url: String,
        pub preview_image_url: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct n720p {
        #[serde(rename = "webMVideoUrl")]
        pub web_mvideo_url: String,
        #[serde(rename = "mp4VideoUrl")]
        pub mp4video_url: String,
        pub preview_image_url: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct n360p {
        #[serde(rename = "webMVideoUrl")]
        pub web_mvideo_url: String,
        #[serde(rename = "mp4VideoUrl")]
        pub mp4video_url: String,
        pub preview_image_url: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct n240p {
        #[serde(rename = "webMVideoUrl")]
        pub web_mvideo_url: String,
        #[serde(rename = "mp4VideoUrl")]
        pub mp4video_url: String,
        pub preview_image_url: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct n540p {
        #[serde(rename = "webMVideoUrl")]
        pub web_mvideo_url: String,
        #[serde(rename = "mp4VideoUrl")]
        pub mp4video_url: String,
        pub preview_image_url: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SubtitlesVtt {
        pub ar: String,
        pub ko: String,
        pub ja: String,
        pub en: String,
        pub it: String,
        pub id: String,
        pub fr: String,
        pub es: String,
    }
}

pub mod supplements {
    use serde::{Deserialize, Serialize};

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Response {
        pub elements: Vec<Element>,
        pub paging: Paging,
        pub linked: Linked,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Element {
        pub item_id: String,
        pub id: String,
        pub course_id: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Paging {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Linked {
        #[serde(rename = "openCourseAssets.v1")]
        pub open_course_assets_v1: Vec<OpenCourseAssetsV1>,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OpenCourseAssetsV1 {
        pub item_id: String,
        pub type_name: String,
        pub definition: Definition,
        pub id: String,
        pub course_id: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Definition {
        pub dtd_id: String,
        pub renderable_html_with_metadata: RenderableHtmlWithMetadata,
        pub value: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RenderableHtmlWithMetadata {
        pub metadata: Metadata,
        pub renderable_html: String,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Metadata {
        pub has_math: bool,
        pub is_plain_text: bool,
        pub has_asset_block: bool,
        pub has_code_block: bool,
    }
}

pub mod enroll_in_course {
    use serde::{Deserialize, Serialize};

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Response {
        pub elements: Vec<Element>,
        pub paging: Paging,
        pub linked: Linked,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Element {
        pub course_role: String,
        pub id: String,
        pub user_id: i64,
        pub course_id: String,
        pub timestamp: i64,
    }

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Paging {}

    #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Linked {}
}
