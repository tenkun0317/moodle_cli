pub const MOODLE_BASE: &str = "https://joho.g-edu.uec.ac.jp/moodle3";
pub const LOGIN_URL: &str = "https://joho.g-edu.uec.ac.jp/moodle3/auth/shibboleth/index.php";
pub const DEBUG_DIR: &str = "debug_pages";

pub struct Course {
    pub id: u32,
    pub name: String,
}

pub struct Section {
    pub id: u32,
    pub name: String,
}

pub enum ActivityType {
    Quiz,
    Assignment,
    Url,
}

pub struct QuizAssignment {
    pub id: u32,
    pub name: String,
    pub activity_type: ActivityType,
}

impl QuizAssignment {
    pub fn is_assignment(&self) -> bool {
        matches!(self.activity_type, ActivityType::Assignment)
    }
    pub fn is_url(&self) -> bool {
        matches!(self.activity_type, ActivityType::Url)
    }
}
