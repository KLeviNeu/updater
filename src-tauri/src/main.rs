use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, State,
};
use tauri::WindowEvent;

// --- DATA STRUCTURES ---

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Instance {
    pub folder: PathBuf,
    pub url: String,
    pub repo_name: String,
}

#[derive(Clone)]
pub struct InstanceManager {
    instances: Arc<Mutex<Vec<Instance>>>,
}

impl InstanceManager {
    /// Loads saved instance pairs from file. If the file doesn't exist, returns an empty manager.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(Self {
                instances: Arc::new(Mutex::new(Vec::new())),
            });
        }

        let file_contents = fs::read_to_string(path)?;
        let instances: Vec<Instance> = serde_json::from_str(&file_contents)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(Self {
            instances: Arc::new(Mutex::new(instances)),
        })
    }

    /// Saves the current list of pairs to disk.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let instances = self.instances.lock().unwrap();
        let json_string = serde_json::to_string_pretty(&*instances)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        fs::write(path, json_string)?;
        Ok(())
    }

    /// Adds or modifies an instance pair and writes the updated list to instances.json.
    pub fn save_and_run_pair<P: AsRef<Path>>(
        &self,
        folder: PathBuf,
        url: String,
        repo_name: String,
        file_path: P,
    ) -> io::Result<()> {
        let instance = Instance {
            folder: folder.clone(),
            url: url.clone(),
            repo_name: repo_name.clone(),
        };

        {
            let mut instances = self.instances.lock().unwrap();

            if let Some(existing) = instances.iter_mut().find(|i| i.folder == folder) {
                existing.url = url;
                existing.repo_name = repo_name;
            } else {
                instances.push(instance.clone());
            }
        }

        self.save_to_file(file_path)?;

        if !instance.url.is_empty() && instance.folder.exists() {
            println!("Saved pair for {:?}. Executing packwiz...", instance.folder);
            let _ = run_packwiz(&instance);
        }

        Ok(())
    }

    /// Deletes an instance by its folder path and updates instances.json.
    pub fn delete_pair<P: AsRef<Path>>(&self, folder: &Path, file_path: P) -> io::Result<()> {
        {
            let mut instances = self.instances.lock().unwrap();
            instances.retain(|instance| instance.folder != folder);
        }

        self.save_to_file(file_path)?;
        println!("Removed instance {:?} from instances.json", folder);
        Ok(())
    }
}

// --- SCANNER STRUCTS & FUNCTIONS ---

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScannedFolder {
    pub folder: PathBuf,
    pub folder_name: String,
    pub has_pack_toml: bool,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ScanResult {
    pub folders: Vec<ScannedFolder>,
}

/// Helper function to locate the default `.minecraft/versions` folder across OSes.
fn get_minecraft_versions_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join(".minecraft").join("versions"))
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("minecraft")
                .join("versions")
        })
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".minecraft").join("versions"))
    }
}

/// Scans the target directory (or default .minecraft/versions) for subfolders.
pub fn scan_versions_directory(override_dir: Option<&Path>) -> io::Result<ScanResult> {
    let target_dir = match override_dir {
        Some(path) if !path.as_os_str().is_empty() => path.to_path_buf(),
        _ => get_minecraft_versions_dir().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Could not determine Minecraft versions directory",
            )
        })?,
    };

    if !target_dir.exists() || !target_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Versions directory does not exist: {:?}", target_dir),
        ));
    }

    let mut result = ScanResult::default();
    let entries = fs::read_dir(target_dir)?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            let folder_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let has_pack_toml = path.join("pack.toml").exists();

            result.folders.push(ScannedFolder {
                folder: path,
                folder_name,
                has_pack_toml,
            });
        }
    }

    Ok(result)
}

fn run_packwiz(instance: &Instance) -> io::Result<()> {
    println!("Updating modpack at {:?}", instance.folder);

    let status = Command::new("packwiz")
        .arg("modpack")
        .arg("update")
        .arg("-y")
        .current_dir(&instance.folder)
        .status()?;

    if status.success() {
        println!("Successfully updated modpack for {:?}", instance.folder);
    } else {
        eprintln!("Packwiz update failed with status: {:?}", status);
    }

    Ok(())
}

// --- TAURI COMMANDS ---

#[tauri::command]
fn get_instances(state: State<'_, InstanceManager>) -> Vec<Instance> {
    let instances = state.instances.lock().unwrap();
    instances.clone()
}

#[tauri::command]
fn add_or_update_instance(
    folder: String,
    url: String,
    repo_name: String,
    json_file: String,
    state: State<'_, InstanceManager>,
) -> Result<(), String> {
    let folder_path = PathBuf::from(folder);
    state
        .save_and_run_pair(folder_path, url, repo_name, json_file)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_instance(
    folder: String,
    json_file: String,
    state: State<'_, InstanceManager>,
) -> Result<(), String> {
    let folder_path = PathBuf::from(folder);
    state
        .delete_pair(&folder_path, json_file)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn scan_instances(custom_path: Option<String>) -> Result<ScanResult, String> {
    let override_path = custom_path.as_deref().map(Path::new);
    scan_versions_directory(override_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn run_packwiz_command(
    folder: String,
    url: String,
    repo_name: String,
) -> Result<(), String> {
    let instance = Instance {
        folder: PathBuf::from(folder),
        url,
        repo_name,
    };

    run_packwiz(&instance).map_err(|e| e.to_string())
}

// --- MAIN FUNCTION ---

fn main() {
    let json_file = "instances.json";

    let manager = InstanceManager::load_from_file(json_file).unwrap_or_else(|_| InstanceManager {
        instances: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
    });

    tauri::Builder::default()
        .manage(manager)
        .setup(|app| {
            // Build system tray menu items
            let show_item = MenuItem::with_id(app, "show", "Open Window", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            // Build system tray icon
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        // Prevent standard close and hide window instead
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_instances,
            add_or_update_instance,
            delete_instance,
            scan_instances,
            run_packwiz_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}