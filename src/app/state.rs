use crate::resolver::ResolvedAsset;

#[derive(Default, Clone)]
pub struct ProgressInfo {
    pub operation: String,
    pub current: String,
    pub done: usize,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub sub_ratio: f64,
    pub done_items: Vec<String>,
}

pub struct ToolItem {
    pub key: String,
    pub selected: bool,
    pub resolved: Option<ResolvedAsset>,
}

#[derive(PartialEq, Clone)]
pub enum ViewState {
    Browsing,
    Installing,
    Completed,
    FilePicker {
        current_dir: std::path::PathBuf,
        entries: Vec<std::path::PathBuf>,
        cursor: usize,
    },
    Restoring,
}

pub enum InstallMsg {
    Progress(String, String, Option<String>),
    SubProgress(f64),
    Log(String),
    Done(String, Result<Vec<String>, String>),
    Finished,
}
