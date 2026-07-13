use axum::{extract::{Path as AxumPath, State}, Json};
use std::{path::Path, sync::Arc, time::Duration};
use openaijsonwrapper::{
    ChatOptions, ContentPart, Message, MessageContent, OpenAIClientBuilder, OpenAIJsonWrapper,
};
use serde_json::json;

use crate::models::{AnalysisJob, AnalysisResult, PhotoAnalysis};
use crate::services::{AnalysisJobUpdate, AppState};

#[derive(serde::Deserialize)]
pub struct StartAnalysisRequest {
    #[serde(default)]
    pub file_paths: Vec<String>,
    pub delay: Option<u64>,
}

#[derive(serde::Deserialize)]
pub struct StartFolderAnalysisRequest {
    pub dir_id: String,
    pub sub_path: Option<String>,
    pub recursive: Option<bool>,
    pub delay: Option<u64>,
}

pub async fn start_analysis(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartAnalysisRequest>,
) -> Result<Json<AnalysisJob>, (axum::http::StatusCode, &'static str)> {
    let valid_paths: Vec<String> = body
        .file_paths
        .into_iter()
        .filter(|p| Path::new(p).exists() && is_image_file(p))
        .collect();

    if valid_paths.is_empty() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "没有有效的图片路径"));
    }

    let job = state.create_analysis_job(valid_paths.len());
    let state_clone = state.clone();
    let job_id = job.job_id.clone();
    let delay_ms = body.delay.unwrap_or(0);
    tokio::spawn(async move {
        run_analysis(state_clone, job_id, valid_paths, delay_ms).await;
    });

    Ok(Json(job))
}

pub async fn start_folder_analysis(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartFolderAnalysisRequest>,
) -> Result<Json<AnalysisJob>, (axum::http::StatusCode, &'static str)> {
    let entry = state
        .get_dir(&body.dir_id)
        .ok_or((axum::http::StatusCode::NOT_FOUND, "目录不存在"))?;

    let base = Path::new(&entry.path).to_path_buf();
    let target = body
        .sub_path
        .as_ref()
        .map(Path::new)
        .unwrap_or(base.as_path())
        .to_path_buf();

    if !target.exists() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "路径不存在"));
    }

    let recursive = body.recursive.unwrap_or(true);
    let mut paths = Vec::new();

    if recursive {
        for entry in walkdir::WalkDir::new(&target).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            if p.is_file() && is_image_file(p.to_string_lossy().as_ref()) {
                paths.push(p.to_string_lossy().to_string());
            }
        }
    } else {
        let read_dir = std::fs::read_dir(&target)
            .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "路径不存在"))?;
        for item in read_dir.filter_map(Result::ok) {
            let p = item.path();
            if p.is_file() && is_image_file(p.to_string_lossy().as_ref()) {
                paths.push(p.to_string_lossy().to_string());
            }
        }
    }

    if paths.is_empty() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "目录下没有图片"));
    }

    let job = state.create_analysis_job(paths.len());
    let state_clone = state.clone();
    let job_id = job.job_id.clone();
    let delay_ms = body.delay.unwrap_or(0);
    tokio::spawn(async move {
        run_analysis(state_clone, job_id, paths, delay_ms).await;
    });

    Ok(Json(job))
}

pub async fn get_analysis_job(
    State(state): State<Arc<AppState>>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<AnalysisJob>, (axum::http::StatusCode, &'static str)> {
    state
        .get_analysis_job(&job_id)
        .map(Json)
        .ok_or((axum::http::StatusCode::NOT_FOUND, "任务不存在"))
}

pub async fn list_results(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AnalysisResult>> {
    Json(state.list_results())
}

pub async fn get_result(
    State(state): State<Arc<AppState>>,
    AxumPath(file_path): AxumPath<String>,
) -> Result<Json<AnalysisResult>, (axum::http::StatusCode, &'static str)> {
    let target = normalize_path(&file_path);
    for r in state.list_results() {
        if normalize_path(&r.file_path) == target {
            return Ok(Json(r));
        }
    }
    Err((axum::http::StatusCode::NOT_FOUND, "结果不存在"))
}

async fn run_analysis(state: Arc<AppState>, job_id: String, paths: Vec<String>, delay_ms: u64) {
    state.update_analysis_job(
        &job_id,
        AnalysisJobUpdate {
            status: Some("running".to_string()),
            progress: None,
            current_file: None,
            results: None,
            finished_at: None,
        },
    );

    let settings = state.get_settings();
    let wrapper = if !settings.api_key.is_empty() {
        let client = OpenAIClientBuilder::new(settings.api_key.clone())
            .base_url(settings.base_url.clone())
            .build();
        Some(OpenAIJsonWrapper::new(
            Box::new(client),
            &settings.model,
            Some(build_target_structure()),
            Some(vec![
                "照片的评价评分需要基于照片的清晰度、构图、色彩和主题等因素综合评定。",
                "请确保输出的 JSON 严格符合指定的结构和类型要求。",
            ]),
            Some("你是一名专业的旅行照片分析师，擅长从图片中分析出丰富的细节和信息。"),
        ))
    } else {
        None
    };

    let mut all_results = Vec::new();
    for (idx, path) in paths.iter().enumerate() {
        let file_name = Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        state.update_analysis_job(
            &job_id,
            AnalysisJobUpdate {
                status: None,
                progress: Some(idx),
                current_file: Some(file_name.clone()),
                results: None,
                finished_at: None,
            },
        );

        let result = if Path::new(path).exists() {
            if let Some(w) = &wrapper {
                analyze_with_wrapper(w, path, &file_name)
            } else {
                AnalysisResult {
                    file_path: path.clone(),
                    file_name,
                    success: false,
                    error: Some("未配置 API Key，请先在设置页保存 api_key/base_url/model".to_string()),
                    data: None,
                    reasoning: None,
                }
            }
        } else {
            AnalysisResult {
                file_path: path.clone(),
                file_name,
                success: false,
                error: Some("文件不存在".to_string()),
                data: None,
                reasoning: None,
            }
        };

        all_results.push(result);
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    state.add_results(all_results.clone());
    state.update_analysis_job(
        &job_id,
        AnalysisJobUpdate {
            status: Some("completed".to_string()),
            progress: Some(paths.len()),
            current_file: None,
            results: Some(all_results),
            finished_at: Some(chrono::Utc::now().to_rfc3339()),
        },
    );
}

fn analyze_with_wrapper(wrapper: &OpenAIJsonWrapper, path: &str, file_name: &str) -> AnalysisResult {
    let messages = vec![Message {
        role: "user".to_string(),
        content: MessageContent::Array(vec![
            ContentPart::Text {
                part_type: "text".to_string(),
                text: "请仔细观察这张图片，按指定 JSON 结构输出。".to_string(),
            },
            ContentPart::ImagePath {
                part_type: "image_path".to_string(),
                image_path: path.to_string(),
            },
        ]),
    }];

    match wrapper.chat(messages, ChatOptions::default()) {
        Ok(chat_result) => {
            if let Some(data_value) = chat_result.data {
                match serde_json::from_value::<PhotoAnalysis>(data_value) {
                    Ok(data) => AnalysisResult {
                        file_path: path.to_string(),
                        file_name: file_name.to_string(),
                        success: true,
                        error: None,
                        data: Some(data),
                        reasoning: if chat_result.reasoning.is_empty() {
                            None
                        } else {
                            Some(chat_result.reasoning)
                        },
                    },
                    Err(e) => AnalysisResult {
                        file_path: path.to_string(),
                        file_name: file_name.to_string(),
                        success: false,
                        error: Some(format!("模型返回结构解析失败: {}", e)),
                        data: None,
                        reasoning: if chat_result.reasoning.is_empty() {
                            None
                        } else {
                            Some(chat_result.reasoning)
                        },
                    },
                }
            } else {
                AnalysisResult {
                    file_path: path.to_string(),
                    file_name: file_name.to_string(),
                    success: false,
                    error: Some(chat_result.error.unwrap_or_else(|| "模型未返回结构化 JSON".to_string())),
                    data: None,
                    reasoning: if chat_result.reasoning.is_empty() {
                        None
                    } else {
                        Some(chat_result.reasoning)
                    },
                }
            }
        }
        Err(e) => AnalysisResult {
            file_path: path.to_string(),
            file_name: file_name.to_string(),
            success: false,
            error: Some(e),
            data: None,
            reasoning: None,
        },
    }
}

fn build_target_structure() -> serde_json::Value {
    json!({
        "score": 0,
        "style": "",
        "caption": "",
        "main_objects": ["", ""],
        "blurry": "",
        "comments": "",
        "recommendations": ""
    })
}

fn is_image_file(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff"
    )
}

fn normalize_path(path: &str) -> String {
    let replaced = path.replace('/', "\\");
    #[cfg(windows)]
    {
        replaced.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        replaced
    }
}
