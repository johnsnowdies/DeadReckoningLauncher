#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    process::exit,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
};

mod app_config;
mod game;
mod styles;
mod updater;

use app_config::{AppConfig, Renderer, ShadowMapSize};
use eframe::egui::{
    self, vec2, Button, ComboBox, FontData, FontDefinitions, FontFamily, IconData, RichText, Stroke, Vec2, ViewportBuilder,
};
use game::Game;
use rfd::MessageDialog;
use styles::Styles;
use updater::{Updater, UpdaterError};

fn show_error(title: &str, desc: &str) {
    MessageDialog::new()
        .set_title(title)
        .set_description(desc)
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn load_icon_data() -> Result<IconData, image::ImageError> {
    let icon_data = include_bytes!("../assets/icon.ico");
    let image = image::load_from_memory(icon_data)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    Ok(IconData { rgba, width, height})
}

fn load_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let open_sans = include_bytes!("../assets/open_sans.ttf");
    let arc_font_data = Arc::new(FontData::from_static(open_sans));

    fonts
        .font_data
        .insert("OpenSans".to_owned(), arc_font_data);

    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "OpenSans".to_owned());

    fonts
}

fn main() -> eframe::Result<()> {
    if !Path::new("launcherconfig.toml").exists() {
        let default_config = AppConfig::default();
        let _ = default_config.write();
    }

    let icon_data = match  load_icon_data() {
        Ok(data) => Arc::new(data),
        Err(_) => {show_error("Icon Error", "Failed to load application icon."); exit(1);},
    };

    let viewport = ViewportBuilder::default()
        .with_maximize_button(false)
        .with_resizable(false)
        .with_inner_size(Vec2 { x: 500.0, y: 225.0 })
        .with_icon(icon_data);

    eframe::run_native(
        "Dead Reckoning",
        eframe::NativeOptions {
            viewport,
            vsync: false,
            centered: true,
            ..Default::default()
        },
        Box::new(|cc| {
            Ok(Box::new(LauncherApp::new(cc)))
        }),
    )
}

#[derive(Debug)]
struct LauncherApp {
    config: AppConfig,
    app_shutdown: bool,
    is_updating: Arc<AtomicBool>,
    new_version: Arc<std::sync::Mutex<Option<String>>>,
    config_update: Arc<std::sync::Mutex<Option<AppConfig>>>,
}

impl LauncherApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load().unwrap_or_else(|err| {
            match err {
                app_config::AppConfigError::ReadFailed => show_error("Read Failed", "Failed to read the configuration file. Please remove 'launcherconfig.toml' and try to launch program again."),
                app_config::AppConfigError::BadStructure => show_error("Bad configuration", "Your configuration seems to be damaged. Please remove 'launcherconfig.toml' and try to launch program again."),
                app_config::AppConfigError::WriteFailed => todo!(),
            };
            exit(1);
        });

        cc.egui_ctx.set_fonts(load_fonts());

        LauncherApp {
            config,
            app_shutdown: false,
            is_updating: Arc::new(AtomicBool::new(false)),
            new_version: Arc::new(std::sync::Mutex::new(None)),
            config_update: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl fmt::Display for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Renderer::DX8 => write!(f, "DirectX 8"),
            Renderer::DX9 => write!(f, "DirectX 9"),
            Renderer::DX10 => write!(f, "DirectX 10"),
            Renderer::DX11 => write!(f, "DirectX 11"),
        }
    }
}

impl fmt::Display for ShadowMapSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShadowMapSize::Size1536 => write!(f, "1536"),
            ShadowMapSize::Size2048 => write!(f, "2048"),
            ShadowMapSize::Size2560 => write!(f, "2560"),
            ShadowMapSize::Size3072 => write!(f, "3072"),
            ShadowMapSize::Size4096 => write!(f, "4096"),
        }
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Проверяем, есть ли обновление конфигурации
        if let Ok(mut config_guard) = self.config_update.lock() {
            if let Some(updated_config) = config_guard.take() {
                // Обновляем основную конфигурацию
                self.config = updated_config;
            }
        }
        
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.visuals().dark_mode {
                ui.style_mut().visuals = Styles::dark();
            } else {
                ui.style_mut().visuals = Styles::light();
            }

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.style_mut().spacing.item_spacing = vec2(0., 38.);
                    
                    ui.vertical(|ui| {
                        ui.style_mut().spacing.item_spacing = vec2(0., 0.);
                        ui.label(RichText::new("Dead Reckoning").size(24.0));
                        ui.horizontal(|ui| {
                            ui.label("Modpack by Eslider");

                        });
                    });
                    
            
                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.item_spacing = vec2(6., 6.);
                        
                        ui.set_min_size(vec2(220., 100.));
                        ui.vertical(|ui| {
                            ui.set_min_size(vec2(150., 100.));
                            ui.label(RichText::new("Renderer"));
                            ComboBox::from_id_salt("renderer")
                                .selected_text(self.config.renderer.to_string())
                                .width(150.)
                                .show_ui(ui, |ui| {
                                    ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                                    ui.selectable_value(&mut self.config.renderer, Renderer::DX8, "DirectX 8");
                                    ui.selectable_value(&mut self.config.renderer, Renderer::DX9, "DirectX 9");
                                    ui.selectable_value(&mut self.config.renderer, Renderer::DX10, "DirectX 10");
                                    ui.selectable_value(&mut self.config.renderer, Renderer::DX11, "DirectX 11");
                                });
                            ui.label(RichText::new("Shadow Map Size"));
                            ComboBox::from_id_salt("shadow_map")
                                .selected_text(self.config.shadow_map.to_string())
                                .width(150.)
                                .show_ui(ui, |ui| {
                                    ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                                    ui.selectable_value(&mut self.config.shadow_map, ShadowMapSize::Size1536, "1536");
                                    ui.selectable_value(&mut self.config.shadow_map, ShadowMapSize::Size2048, "2048");
                                    ui.selectable_value(&mut self.config.shadow_map, ShadowMapSize::Size2560, "2560");
                                    ui.selectable_value(&mut self.config.shadow_map, ShadowMapSize::Size3072, "3072");
                                    ui.selectable_value(&mut self.config.shadow_map, ShadowMapSize::Size4096, "4096");
                                });
                                // Отображаем версию, если она есть
                            // Сначала проверяем, есть ли новая версия
                            let version_to_display = if let Ok(version_guard) = self.new_version.lock() {
                                if let Some(ref new_ver) = *version_guard {
                                    new_ver.clone()
                                } else if let Some(ref current_ver) = self.config.version {
                                    current_ver.clone()
                                } else {
                                    "Unknown".to_string()
                                }
                            } else if let Some(ref current_ver) = self.config.version {
                                current_ver.clone()
                            } else {
                                "Unknown".to_string()
                            };
                            
                            ui.label(format!("Version: {}", version_to_display));
                        });
                        ui.vertical(|ui| {
                            ui.set_min_size(vec2(150., 100.));
                            ui.label(RichText::new("Misc settings"));
                            ui.checkbox(&mut self.config.debug, "Debug Mode");
                            ui.checkbox(&mut self.config.prefetch_sounds, "Prefetch Sounds");
                            ui.checkbox(&mut self.config.use_avx, "Use AVX");
                        });
                        
                    });

                   
                    
                });
                ui.vertical(|ui| {
                    let play_button = ui.add_sized([180., 65.], Button::new("Play"));
                    
                    // Добавляем кнопку обновления, если настроен URL
                    if self.config.update_url.is_some() {
                        let update_text = if self.is_updating.load(Ordering::Relaxed) {
                            "Updating..."
                        } else {
                            "Check for Updates"
                        };
                        
                        let update_button = ui.add_sized([180., 35.], Button::new(update_text));
                        if update_button.clicked() && !self.is_updating.load(Ordering::Relaxed) {
                            self.is_updating.store(true, Ordering::Relaxed);
                            
                            // Запускаем процесс обновления в отдельном потоке
                            let config_clone = self.config.clone();
                            let ctx_clone = ctx.clone();
                            let is_updating_clone = self.is_updating.clone();
                            let new_version_clone = self.new_version.clone();
                            let config_update_clone = self.config_update.clone();
                            
                            std::thread::spawn(move || {
                                match Updater::new(config_clone.clone()) {
                                    Ok(mut updater) => {
                                        let result = updater.update(|_progress| {
                                            // Обновляем UI при изменении прогресса
                                            ctx_clone.request_repaint();
                                        });
                                        
                                        // Сбрасываем флаг обновления
                                        is_updating_clone.store(false, Ordering::Relaxed);
                                        ctx_clone.request_repaint();
                                        
                                        match result {
                                            Ok(new_version) => {
                                                // Обновляем версию в конфигурации
                                                let mut updated_config = config_clone.clone();
                                                updated_config.version = Some(new_version.clone());
                                                
                                                // Обновляем разделяемое значение версии
                                                if let Ok(mut version_guard) = new_version_clone.lock() {
                                                    *version_guard = Some(new_version.clone());
                                                }
                                                
                                                // Сохраняем обновленную конфигурацию для главного потока
                                                if let Ok(mut config_guard) = config_update_clone.lock() {
                                                    *config_guard = Some(updated_config.clone());
                                                }
                                                
                                                // Сохраняем обновленную конфигурацию в файл
                                                if let Err(_e) = updated_config.write() {
                                                    MessageDialog::new()
                                                        .set_title("Configuration Save Error")
                                                        .set_description(format!("Failed to save updated configuration:"))
                                                        .set_level(rfd::MessageLevel::Error)
                                                        .set_buttons(rfd::MessageButtons::Ok)
                                                        .show();
                                                }
                                                
                                                // Обновление успешно завершено
                                                MessageDialog::new()
                                                    .set_title("Update Complete")
                                                    .set_description(format!("Successfully updated to version {}", new_version))
                                                    .set_level(rfd::MessageLevel::Info)
                                                    .set_buttons(rfd::MessageButtons::Ok)
                                                    .show();
                                            },
                                            Err(UpdaterError::NoUpdatesAvailable) => {
                                                MessageDialog::new()
                                                    .set_title("No Updates Available")
                                                    .set_description("You are already running the latest version.")
                                                    .set_level(rfd::MessageLevel::Info)
                                                    .set_buttons(rfd::MessageButtons::Ok)
                                                    .show();
                                            },
                                            Err(e) => {
                                                MessageDialog::new()
                                                    .set_title("Update Failed")
                                                    .set_description(format!("Failed to update: {}", e))
                                                    .set_level(rfd::MessageLevel::Error)
                                                    .set_buttons(rfd::MessageButtons::Ok)
                                                    .show();
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        MessageDialog::new()
                                            .set_title("Update Error")
                                            .set_description(format!("Failed to initialize updater: {}", e))
                                            .set_level(rfd::MessageLevel::Error)
                                            .set_buttons(rfd::MessageButtons::Ok)
                                            .show();
                                        
                                        // Сбрасываем флаг обновления
                                        is_updating_clone.store(false, Ordering::Relaxed);
                                        ctx_clone.request_repaint();
                                    }
                                }
                            });
                        }
                    }
                    
                    let clear_button = ui.add_sized([180., 35.], Button::new("Clear Shader Cache"));
                    let about_button = ui.add_sized([180., 35.], Button::new("About Launcher"));
                    let quit_button = ui.add_sized([180., 35.], Button::new("Quit"));
                    if play_button.clicked() {
                        println!("{:?}", self);
                        let game = Game::new(self.config.renderer, self.config.use_avx);
                        let mut args: Vec<String> = Vec::new();
                        let shadows_arg: String = match self.config.shadow_map {
                            ShadowMapSize::Size1536 => "-smap1536".to_string(),
                            ShadowMapSize::Size2048 => "-smap2048".to_string(),
                            ShadowMapSize::Size2560 => "-smap2560".to_string(),
                            ShadowMapSize::Size3072 => "-smap3072".to_string(),
                            ShadowMapSize::Size4096 => "-smap4096".to_string(),
                        };
                        args.push(shadows_arg);
                        if self.config.debug {
                            args.push("-dbg".to_string());
                        }

                        if self.config.prefetch_sounds {
                            args.push("-prefetch_sounds".to_string());
                        }
                        let launch_result = game.launch(args);
                        if let Err(e) = launch_result {
                            match e {
                                game::GameError::ExecutableNotFound => {
                                    MessageDialog::new()
                                        .set_title("Executable not found")
                                        .set_description("Could not find the executable file of the game. Make sure you run the launcher from the game folder.")
                                        .set_level(rfd::MessageLevel::Error)
                                        .set_buttons(rfd::MessageButtons::Ok)
                                        .show();
                                },
                                game::GameError::Unknown(i) => {
                                    MessageDialog::new()
                                        .set_title("Unknown error occured")
                                        .set_description(format!("The launcher failed to launch the game due to an unexpected error: {}",i))
                                        .set_level(rfd::MessageLevel::Error)
                                        .set_buttons(rfd::MessageButtons::Ok)
                                        .show();
                                },
                            }
                        } else {
                            self.app_shutdown = true;
                        }
                    }

                    if clear_button.clicked() {
                        let mut cache_path: PathBuf = env::current_dir().unwrap();
                        cache_path.push("appdata\\shaders_cache");
                        println!("{:?}", cache_path);
                        if !cache_path.exists() {
                            let _ = MessageDialog::new()
                            .set_title("Path not found")
                            .set_description("The launcher cannot find the shader cache folder. Make sure you run the launcher in the Anomaly game folder.")
                            .set_level(rfd::MessageLevel::Error)
                            .set_buttons(rfd::MessageButtons::Ok)
                            .show();
                        } else {
                            fs::remove_dir_all(cache_path.clone()).unwrap();
                            fs::create_dir(cache_path.clone()).unwrap();
                            MessageDialog::new()
                            .set_title("Clear Shader Cache")
                            .set_description("Shader cache has been deleted.")
                            .set_level(rfd::MessageLevel::Info)
                            .set_buttons(rfd::MessageButtons::Ok)
                            .show();
                        }
                    }

                    if about_button.clicked() {
                        MessageDialog::new()
                        .set_title("About Launcher")
                        .set_buttons(rfd::MessageButtons::Ok)
                        .set_level(rfd::MessageLevel::Info)
                        .set_description(r#"Anomaly Launcher for S.T.A.L.K.E.R Anomaly 1.5.1 and above.

Made by Konstantin "ZERO" Zhigaylo (@kostya_zero). 
This software has open source code on GitHub.

https://github.com/kostya-zero/AnomalyLauncher"#).show();
                    }

                    if quit_button.clicked() {
                        self.app_shutdown = true;
                    }
                });
            });
        });

        // Handle close via close button
        if ctx.input(|i| i.viewport().close_requested()) {
            self.app_shutdown = true;
        }

        if self.app_shutdown {
            match self.config.write() {
                Ok(_) => {},
                Err(_) => show_error("Write Failed", "Failed to write data to configuration file. You might need to set your options again."),
            };
            exit(0);
        }
    }
}
