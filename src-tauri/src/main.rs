use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::State;

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

    /// Saves the current list of pairs to disk, creating instances.json if it doesn't exist.
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

    /// Returns matching instances for a repository name.
    pub fn get_by_repo(&self, repo_name: &str) -> Vec<Instance> {
        let instances = self.instances.lock().unwrap();
        instances
            .iter()
            .filter(|i| i.repo_name.eq_ignore_ascii_case(repo_name))
            .cloned()
            .collect()
    }
}

// --- SCANNER STRUCTS & FUNCTIONS ---

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScannedRepo {
    pub folder: PathBuf,
    pub remote_url: String,
    pub repo_name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScannedFolder {
    pub folder: PathBuf,
    pub folder_name: String,
    pub has_pack_toml: bool,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ScanResult {
    pub repos: Vec<ScannedRepo>,
    pub folders: Vec<ScannedFolder>,
}

pub fn scan_directory(root_dir: &Path) -> io::Result<ScanResult> {
    if !root_dir.exists() || !root_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Root directory does not exist or is not a folder",
        ));
    }

    let mut result = ScanResult::default();
    let entries = fs::read_dir(root_dir)?;

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
                folder: path.clone(),
                folder_name,
                has_pack_toml,
            });

            if path.join(".git").exists() {
                if let Ok(git_info) = extract_git_info(&path) {
                    result.repos.push(ScannedRepo {
                        folder: path,
                        remote_url: git_info.remote_url,
                        repo_name: git_info.repo_name,
                    });
                }
            }
        }
    }

    Ok(result)
}

struct GitInfo {
    remote_url: String,
    repo_name: String,
}

fn extract_git_info(folder: &Path) -> io::Result<GitInfo> {
    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(folder)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to get git remote URL",
        ));
    }

    let remote_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let repo_name = remote_url
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".git")
        .to_string();

    Ok(GitInfo {
        remote_url,
        repo_name,
    })
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
fn scan_instances(
    root_dir: String,
    json_file: String,
    state: State<'_, InstanceManager>,
) -> Result<ScanResult, String> {
    let root = Path::new(&root_dir);
    let results = scan_directory(root).map_err(|e| e.to_string())?;

    for repo in &results.repos {
        let _ = state.save_and_run_pair(
            repo.folder.clone(),
            repo.remote_url.clone(),
            repo.repo_name.clone(),
            &json_file,
        );
    }

    Ok(results)
}

fn main() {
    let json_file = "instances.json";

    let manager = InstanceManager::load_from_file(json_file).unwrap_or_else(|_| InstanceManager {
        instances: Arc::new(Mutex::new(Vec::new())),
    });

    tauri::Builder::default()
        .manage(manager)
        .invoke_handler(tauri::generate_handler![
            get_instances,
            add_or_update_instance,
            delete_instance,
            scan_instances
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}