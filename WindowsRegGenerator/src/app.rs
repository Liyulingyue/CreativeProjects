use std::path::PathBuf;

use eframe::egui;

use crate::elevate;
use crate::regfile;
use crate::registry::{self, FoundKey};
use crate::scheme::{validate_exe_path, validate_scheme, ProtocolConfig, Scope};

pub struct RegGeneratorApp {
    scheme: String,
    exe_path: String,
    scope: Scope,
    friendly_name: String,
    include_icon: bool,
    include_friendly_name: bool,
    prefix_match: bool,
    status: String,
    status_is_error: bool,
    found: Vec<FoundKey>,
    pending_delete: bool,
    log: Vec<String>,
    last_saved: Option<String>,
    elevated: bool,
    preview: String,
    preview_error: Option<String>,
    preview_signature: String,
}

impl Default for RegGeneratorApp {
    fn default() -> Self {
        Self {
            scheme: "myapp".to_string(),
            exe_path: String::new(),
            scope: Scope::CurrentUser,
            friendly_name: String::new(),
            include_icon: true,
            include_friendly_name: false,
            prefix_match: false,
            status: "就绪".to_string(),
            status_is_error: false,
            found: Vec::new(),
            pending_delete: false,
            log: Vec::new(),
            last_saved: None,
            elevated: registry::is_elevated(),
            preview: String::new(),
            preview_error: None,
            preview_signature: String::new(),
        }
    }
}

impl RegGeneratorApp {
    fn push_log(&mut self, message: impl Into<String>) {
        self.log.push(message.into());
        if self.log.len() > 500 {
            self.log.remove(0);
        }
    }

    fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = message.into();
        self.status_is_error = is_error;
    }

    fn build_config(&self) -> Result<ProtocolConfig, String> {
        let scheme = self.scheme.trim().to_string();
        validate_scheme(&scheme)?;

        let exe_path = PathBuf::from(self.exe_path.trim());
        validate_exe_path(&exe_path)?;

        let friendly_name = if self.include_friendly_name {
            let name = self.friendly_name.trim().to_string();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        } else {
            None
        };

        Ok(ProtocolConfig {
            scheme,
            exe_path,
            scope: self.scope,
            friendly_name,
            include_icon: self.include_icon,
        })
    }

    fn preview_text(&self) -> Result<String, String> {
        let config = self.build_config()?;
        Ok(regfile::render(&config.to_entries()))
    }

    fn refresh_preview(&mut self) {
        let signature = format!(
            "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
            self.scheme,
            self.exe_path,
            self.scope.classes_root(),
            self.friendly_name,
            self.include_friendly_name,
            self.include_icon
        );
        if signature == self.preview_signature {
            return;
        }
        self.preview_signature = signature;
        match self.preview_text() {
            Ok(text) => {
                self.preview = text;
                self.preview_error = None;
            }
            Err(err) => {
                self.preview.clear();
                self.preview_error = Some(err);
            }
        }
    }

    fn pick_exe(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("选择要注册为协议处理器的 EXE")
            .add_filter("可执行文件", &["exe"])
            .pick_file()
        {
            self.exe_path = path.display().to_string();
            self.push_log(format!("已选择 EXE：{}", self.exe_path));
        }
    }

    fn save_reg_file(&mut self) {
        let entries = match self.build_config() {
            Ok(config) => config.to_entries(),
            Err(err) => {
                self.set_status(err, true);
                return;
            }
        };

        let default_name = format!("{}.reg", self.scheme.trim());
        let Some(path) = rfd::FileDialog::new()
            .set_title("保存 .reg 注册表文件")
            .add_filter("注册表文件", &["reg"])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };

        match regfile::write_reg_file(&path, &entries) {
            Ok(()) => {
                let shown = path.display().to_string();
                self.last_saved = Some(shown.clone());
                self.push_log(format!("已生成注册表文件：{shown}"));
                self.set_status(format!("已保存：{shown}"), false);
            }
            Err(err) => {
                let message = format!("保存失败：{err}");
                self.push_log(message.clone());
                self.set_status(message, true);
            }
        }
    }

    fn copy_preview(&mut self, ui: &egui::Ui) {
        self.refresh_preview();
        if let Some(err) = self.preview_error.clone() {
            self.set_status(err, true);
            return;
        }
        ui.ctx().copy_text(self.preview.clone());
        self.push_log("已复制 .reg 文本到剪贴板");
        self.set_status("已复制到剪贴板", false);
    }

    fn scan_registry(&mut self) {
        let scheme = self.scheme.trim().to_string();
        if let Err(err) = validate_scheme(&scheme) {
            self.set_status(err, true);
            return;
        }

        match registry::scan(&scheme, self.prefix_match) {
            Ok(found) => {
                self.push_log(format!(
                    "扫描完成，命中 {} 个键（前缀匹配：{}）",
                    found.len(),
                    self.prefix_match
                ));
                for item in &found {
                    self.push_log(format!("  命中 {}", item.full_path));
                }
                self.pending_delete = false;
                self.found = found;
                self.set_status("扫描完成", false);
            }
            Err(err) => {
                let message = format!("扫描失败：{}", registry::error_message(&err));
                self.push_log(message.clone());
                self.set_status(message, true);
            }
        }
    }

    fn delete_selected(&mut self) {
        let targets: Vec<(registry::Hive, String, String)> = self
            .found
            .iter()
            .filter(|item| item.selected)
            .map(|item| (item.hive, item.key_name.clone(), item.full_path.clone()))
            .collect();

        if targets.is_empty() {
            self.set_status("没有选中任何键", true);
            return;
        }

        let mut succeeded = 0usize;
        let mut failed = 0usize;
        for (hive, key_name, full_path) in targets {
            match registry::remove(hive, &key_name) {
                Ok(()) => {
                    succeeded += 1;
                    self.push_log(format!("已删除 {full_path}"));
                }
                Err(err) => {
                    failed += 1;
                    self.push_log(format!(
                        "删除失败 {full_path}：{}",
                        registry::error_message(&err)
                    ));
                }
            }
        }

        self.pending_delete = false;
        self.found.clear();
        let message = format!("清理完成：成功 {succeeded} 项，失败 {failed} 项");
        self.push_log(message.clone());
        self.set_status(message, failed > 0);
    }

    fn relaunch_elevated(&mut self) {
        match elevate::relaunch_as_admin() {
            Ok(()) => {
                self.push_log("已请求以管理员身份重启");
                self.set_status("已请求以管理员身份重启", false);
            }
            Err(err) => {
                let message = format!("提权重启失败：{err}");
                self.push_log(message.clone());
                self.set_status(message, true);
            }
        }
    }

    fn section_target(&mut self, ui: &mut egui::Ui) {
        ui.heading("① 注册目标");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("协议前缀");
            ui.add(
                egui::TextEdit::singleline(&mut self.scheme)
                    .desired_width(200.0)
                    .hint_text("myapp"),
            );
            ui.label(format!("→ {}://", self.scheme.trim()));
        });

        if let Err(err) = validate_scheme(self.scheme.trim()) {
            ui.colored_label(egui::Color32::from_rgb(200, 80, 60), format!("⚠ {err}"));
        }

        ui.horizontal(|ui| {
            ui.label("EXE 路径");
            ui.add(
                egui::TextEdit::singleline(&mut self.exe_path)
                    .desired_width(360.0)
                    .hint_text(r"C:\Program Files\App\app.exe"),
            );
            if ui.button("浏览…").clicked() {
                self.pick_exe();
            }
        });

        if !self.exe_path.trim().is_empty() {
            if let Err(err) = validate_exe_path(PathBuf::from(self.exe_path.trim()).as_path()) {
                ui.colored_label(egui::Color32::from_rgb(200, 80, 60), format!("⚠ {err}"));
            }
        }

        ui.horizontal(|ui| {
            ui.label("注册范围");
            ui.selectable_value(&mut self.scope, Scope::CurrentUser, "当前用户 (HKCU)");
            ui.selectable_value(&mut self.scope, Scope::AllUsers, "所有用户 (HKLM)");
            ui.label("HKLM 写入与删除需要管理员权限");
        });

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.include_icon, "写入 DefaultIcon");
            ui.checkbox(&mut self.include_friendly_name, "写入 FriendlyTypeName");
        });

        ui.horizontal(|ui| {
            ui.label("友好名称");
            ui.add(
                egui::TextEdit::singleline(&mut self.friendly_name)
                    .desired_width(260.0)
                    .hint_text("我的应用"),
            );
            ui.label("（浏览器中展示该协议的名称）");
        });
    }

    fn section_generate(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("② 生成 .reg 文件");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button("保存为 .reg…").clicked() {
                self.save_reg_file();
            }
            if ui.button("复制内容").clicked() {
                self.copy_preview(ui);
            }
            if let Some(saved) = self.last_saved.clone() {
                if ui.button("打开所在目录").clicked() {
                    let _ = std::process::Command::new("explorer")
                        .arg(format!("/select,\"{saved}\""))
                        .spawn();
                }
            }
        });

        ui.add_space(4.0);
        if let Some(err) = &self.preview_error {
            ui.colored_label(egui::Color32::from_rgb(200, 80, 60), format!("⚠ {err}"));
        } else {
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(&self.preview).monospace()).wrap());
                });
        }
    }

    fn section_clean(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("③ 清理本机已注册的前缀");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.prefix_match, "同时匹配以该前缀开头的其它协议");
            if ui.button("扫描注册表").clicked() {
                self.scan_registry();
            }
            if !self.elevated && ui.button("以管理员身份重启").clicked() {
                self.relaunch_elevated();
            }
        });

        if self.found.is_empty() {
            ui.label("尚未扫描到相关键");
        } else {
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for item in &mut self.found {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut item.selected, "");
                            ui.label(item.full_path.as_str());
                            if item.has_url_protocol {
                                ui.colored_label(
                                    egui::Color32::from_rgb(90, 170, 110),
                                    "URL Protocol",
                                );
                            }
                        });
                        if let Some(command) = &item.command {
                            ui.label(
                                egui::RichText::new(format!("  ↳ {command}"))
                                    .monospace()
                                    .small()
                                    .color(egui::Color32::from_gray(150)),
                            );
                        }
                    }
                });

            let selected = self.found.iter().filter(|item| item.selected).count();
            ui.horizontal(|ui| {
                if ui.button(format!("删除选中的 {selected} 项")).clicked() {
                    self.pending_delete = true;
                }
                if ui.button("全选").clicked() {
                    for item in &mut self.found {
                        item.selected = true;
                    }
                }
                if ui.button("全不选").clicked() {
                    for item in &mut self.found {
                        item.selected = false;
                    }
                }
            });

            if self.pending_delete {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 80, 60),
                        "确认删除所选注册表键？该操作不可撤销。",
                    );
                    if ui.button("确认删除").clicked() {
                        self.delete_selected();
                    }
                    if ui.button("取消").clicked() {
                        self.pending_delete = false;
                    }
                });
            }
        }
    }

    fn section_log(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("运行日志");
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.log.is_empty() {
                    ui.label("暂无日志");
                }
                for line in &self.log {
                    ui.label(egui::RichText::new(line.as_str()).monospace().small());
                }
            });
    }
}

impl eframe::App for RegGeneratorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.refresh_preview();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("WindowsRegGenerator");
                ui.label(
                    egui::RichText::new("URL 协议处理器注册表生成器")
                        .color(egui::Color32::from_gray(140)),
                );
            });

            let color = if self.status_is_error {
                egui::Color32::from_rgb(200, 80, 60)
            } else {
                egui::Color32::from_rgb(90, 170, 110)
            };
            ui.colored_label(color, self.status.clone());

            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                self.section_target(ui);
                self.section_generate(ui);
                self.section_clean(ui);
                self.section_log(ui);
            });
        });
    }
}
