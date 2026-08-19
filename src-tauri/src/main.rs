#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use tauri::{Manager, State};

const BOOTSTRAP_URL: &str = "https://github.com/packwiz/packwiz-installer-bootstrap/releases/download/v0.0.3/packwiz-installer-bootstrap.jar";
const BOOTSTRAP_FILE_NAME: &str = "packwiz-installer-bootstrap.jar";

// --- DATA STRUCTURES ---

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Instance {
    pub folder: PathBuf,
    pub url: String,
    pub repo_name: String,
}

#[derive(serde::Serialize)]
pub struct FolderInfo {
    pub folder: String,
    pub folder_name: String,
    pub has_pack_toml: bool,
}

#[derive(serde::Serialize)]
pub struct ScanResult {
    pub folders: Vec<FolderInfo>,
}

#[derive(Clone)]
pub struct InstanceManager {
    instances: Arc<Mutex<Vec<Instance>>>,
    app_data_dir: PathBuf,
}

impl InstanceManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&app_data_dir);

        let json_path = app_data_dir.join("instances.json");
        let instances = Self::load_instances(&json_path).unwrap_or_default();

        Self {
            instances: Arc::new(Mutex::new(instances)),
            app_data_dir,
        }
    }

    fn load_instances<P: AsRef<Path>>(path: P) -> io::Result<Vec<Instance>> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file_contents = fs::read_to_string(path)?;
        serde_json::from_str(&file_contents)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn save_to_file(&self) -> io::Result<()> {
        let json_path = self.app_data_dir.join("instances.json");
        let instances = self.instances.lock().unwrap();
        let json_string = serde_json::to_string_pretty(&*instances)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        fs::write(json_path, json_string)?;
        Ok(())
    }

    /// Checks if packwiz-installer-bootstrap.jar exists in AppData, downloading it if absent.
    pub fn ensure_bootstrap_jar(&self) -> io::Result<PathBuf> {
        let target_path = self.app_data_dir.join(BOOTSTRAP_FILE_NAME);

        if !target_path.exists() {
            println!("Downloading packwiz-installer-bootstrap.jar...");

            let response = ureq::get(BOOTSTRAP_URL)
                .call()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

            let mut reader = response.into_body().into_reader();
            let mut file = File::create(&target_path)?;

            io::copy(&mut reader, &mut file)?;

            println!("Downloaded packwiz-installer-bootstrap.jar successfully.");
        }

        Ok(target_path)
    }

    /// Copies the bootstrap JAR into the instance directory.
    fn copy_bootstrap_to_instance(&self, target_folder: &Path) -> io::Result<PathBuf> {
        let source_jar = self.ensure_bootstrap_jar()?;
        let destination_jar = target_folder.join(BOOTSTRAP_FILE_NAME);

        fs::copy(&source_jar, &destination_jar)?;
        Ok(destination_jar)
    }

    pub fn save_and_run_pair(
        &self,
        folder: PathBuf,
        url: String,
        repo_name: String,
    ) -> io::Result<()> {
        fs::create_dir_all(&folder)?;

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

        self.save_to_file()?;

        if !instance.url.is_empty() && instance.folder.exists() {
            let jar_path = self.copy_bootstrap_to_instance(&instance.folder)?;
            run_bootstrap_installer(&jar_path, &instance.folder, &instance.url)?;
        }

        Ok(())
    }

    pub fn delete_pair(&self, folder: &Path) -> io::Result<()> {
        {
            let mut instances = self.instances.lock().unwrap();
            instances.retain(|instance| instance.folder != folder);
        }

        self.save_to_file()?;
        Ok(())
    }
}

// Executes: java -jar packwiz-installer-bootstrap.jar <URL>
fn run_bootstrap_installer(jar_path: &Path, working_dir: &Path, pack_url: &str) -> io::Result<()> {
    println!("Running packwiz-installer-bootstrap for target: {}", pack_url);

    let status = Command::new("java")
        .arg("-jar")
        .arg(jar_path)
        .arg(pack_url)
        .current_dir(working_dir)
        .status()?;

    if status.success() {
        println!("Successfully installed modpack at {:?}", working_dir);
    } else {
        eprintln!("Bootstrap installer exited with error code: {:?}", status);
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
    state: State<'_, InstanceManager>,
) -> Result<(), String> {
    let folder_path = PathBuf::from(folder);
    state
        .save_and_run_pair(folder_path, url, repo_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_instance(folder: String, state: State<'_, InstanceManager>) -> Result<(), String> {
    let folder_path = PathBuf::from(folder);
    state.delete_pair(&folder_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn scan_instances(custom_path: Option<String>) -> Result<ScanResult, String> {
    let base_path = match custom_path {
        Some(p) => PathBuf::from(p),
        None => {
            let appdata = std::env::var("APPDATA").map_err(|_| "Could not find APPDATA".to_string())?;
            PathBuf::from(appdata).join(".minecraft").join("versions")
        }
    };

    let mut folders = Vec::new();

    if base_path.exists() && base_path.is_dir() {
        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let folder_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let has_pack_toml = path.join("pack.toml").exists();

                    folders.push(FolderInfo {
                        folder: path.to_string_lossy().to_string(),
                        folder_name,
                        has_pack_toml,
                    });
                }
            }
        }
    }

    Ok(ScanResult { folders })
}

#[tauri::command]
fn run_packwiz_all(state: State<'_, InstanceManager>) -> Result<(), String> {
    let instances = state.instances.lock().unwrap().clone();

    for instance in instances {
        if !instance.url.is_empty() && instance.folder.exists() {
            let jar_path = state
                .copy_bootstrap_to_instance(&instance.folder)
                .map_err(|e| e.to_string())?;

            run_bootstrap_installer(&jar_path, &instance.folder, &instance.url)
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data directory");

            let manager = InstanceManager::new(app_data_dir);
            app.manage(manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_instances,
            add_or_update_instance,
            delete_instance,
            scan_instances,
            run_packwiz_all
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}