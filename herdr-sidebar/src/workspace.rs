//! Read-only discovery of the folders explicitly selected by a VS Code workspace.
//! Task registrations establish identity; the repository registry supplies targets.
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Workspace {
    pub file: PathBuf,
    pub control_root: PathBuf,
    pub folders: Vec<Folder>,
    /// Number of repos declared by each selected task, including repos not in the workspace file.
    pub declared: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct Folder {
    pub label: String,
    pub path: PathBuf,
    pub task: Option<String>,
    pub repo_id: Option<String>,
    pub target: Option<Target>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub remote_url: String,
    pub branch: String,
}

#[derive(Deserialize)]
struct WorkspaceFile {
    folders: Vec<WorkspaceFolder>,
}

#[derive(Deserialize)]
struct WorkspaceFolder {
    name: Option<String>,
    path: Option<String>,
    uri: Option<String>,
}

#[derive(Deserialize)]
struct TaskFile {
    id: String,
    workspace: Option<TaskWorkspace>,
}

#[derive(Deserialize)]
struct TaskWorkspace {
    repos: Vec<String>,
    worktrees: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Registry {
    repositories: Vec<RegistryEntry>,
}

#[derive(Deserialize)]
struct RegistryEntry {
    id: String,
    remote: Option<String>,
    default_branch: Option<String>,
}

struct Registration {
    task: String,
    repo: String,
    declared: usize,
}

struct Registrations {
    by_path: HashMap<PathBuf, Vec<Registration>>,
    invalid_tasks: HashMap<String, String>,
    issue: Option<String>,
}

/// Discover the nearest ancestor's workspace. An explicit path takes precedence;
/// an ambiguous directory is an error, never an arbitrary choice.
pub fn discover(cwd: &Path) -> Result<Option<Workspace>, String> {
    let cwd = fs::canonicalize(cwd)
        .map_err(|e| format!("cannot resolve working directory {}: {e}", cwd.display()))?;
    let file = match env::var_os("HERDR_SIDEBAR_CODE_WORKSPACE") {
        Some(value) => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        }
        None => match find_workspace(&cwd)? {
            Some(file) => file,
            None => return Ok(None),
        },
    };
    if !file.is_file() {
        return Err(format!(
            "workspace {} is missing or not a file",
            file.display()
        ));
    }
    // Resolve the containing directory, not the file symlink: relative folders
    // belong to the workspace's selected location, not the symlink's target.
    let parent = file
        .parent()
        .ok_or("workspace file has no parent directory")?;
    let control_root = fs::canonicalize(parent).map_err(|e| {
        format!(
            "cannot resolve workspace directory {}: {e}",
            parent.display()
        )
    })?;
    let file = control_root.join(file.file_name().ok_or("workspace file has no name")?);
    let text = fs::read_to_string(&file)
        .map_err(|e| format!("cannot read workspace {}: {e}", file.display()))?;
    let spec: WorkspaceFile =
        json5::from_str(&text).map_err(|e| format!("invalid workspace {}: {e}", file.display()))?;
    let registrations = read_registrations(&control_root);
    let registry = read_registry(&control_root);
    let mut folders = Vec::with_capacity(spec.folders.len());
    let mut seen = HashSet::new();
    let mut declared = BTreeMap::new();
    for (index, entry) in spec.folders.into_iter().enumerate() {
        let fallback_label = entry.path.as_deref().unwrap_or("<no path>").to_string();
        let label = entry
            .name
            .filter(|name| !name.is_empty())
            .unwrap_or(fallback_label);
        let mut folder = Folder {
            label,
            path: control_root.clone(),
            task: None,
            repo_id: None,
            target: None,
            error: None,
        };
        let Some(relative) = entry.path else {
            folder.error = Some(if entry.uri.is_some() {
                "workspace URI folders are not local paths".into()
            } else {
                format!("folder {} has no path", index + 1)
            });
            folders.push(folder);
            continue;
        };
        if relative.is_empty() {
            folder.error = Some("workspace folder path is empty".into());
            folders.push(folder);
            continue;
        }
        let selected = control_root.join(relative);
        // Canonical paths identify one physical checkout even across symlink aliases.
        folder.path = normalized(&selected);
        if !seen.insert(folder.path.clone()) {
            continue;
        }
        if !folder.path.is_dir() {
            folder.error = Some(format!(
                "selected folder is missing or not a directory: {}",
                selected.display()
            ));
        }
        if folder.path != control_root {
            match registrations.by_path.get(&folder.path) {
                Some(matches) if matches.len() == 1 => {
                    let registration = &matches[0];
                    folder.task = Some(registration.task.clone());
                    folder.repo_id = Some(registration.repo.clone());
                    declared.insert(registration.task.clone(), registration.declared);
                    match &registry {
                        Ok(entries) => match entries.get(&registration.repo) {
                            Some(Ok(target)) => folder.target = Some(target.clone()),
                            Some(Err(error)) => add_error(&mut folder, error.clone()),
                            None => add_error(
                                &mut folder,
                                format!(
                                    "repository {} is absent from config/repositories.yaml",
                                    registration.repo
                                ),
                            ),
                        },
                        Err(error) => add_error(&mut folder, error.clone()),
                    }
                }
                Some(_) => add_error(
                    &mut folder,
                    "selected folder is registered by multiple tasks".into(),
                ),
                None => {
                    let error = task_slug(&folder.path, &control_root)
                        .and_then(|slug| registrations.invalid_tasks.get(slug))
                        .or(registrations.issue.as_ref())
                        .cloned()
                        .unwrap_or_else(|| {
                            "selected folder has no exact task.json worktree registration".into()
                        });
                    add_error(&mut folder, error);
                }
            }
        }
        folders.push(folder);
    }
    Ok(Some(Workspace {
        file,
        control_root,
        folders,
        declared,
    }))
}

fn find_workspace(cwd: &Path) -> Result<Option<PathBuf>, String> {
    for dir in cwd.ancestors() {
        let entries =
            fs::read_dir(dir).map_err(|e| format!("cannot inspect {}: {e}", dir.display()))?;
        let mut candidates = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot inspect {}: {e}", dir.display()))?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "code-workspace") && path.is_file() {
                candidates.push(path);
            }
        }
        if candidates.is_empty() {
            continue;
        }
        let expected = dir
            .file_name()
            .map(|name| format!("{}.code-workspace", name.to_string_lossy()));
        if let Some(preferred) = candidates.iter().find(|candidate| {
            candidate
                .file_name()
                .is_some_and(|name| expected.as_deref() == name.to_str())
        }) {
            return Ok(Some(preferred.clone()));
        }
        if candidates.len() == 1 {
            return Ok(candidates.pop());
        }
        return Err(format!(
            "multiple .code-workspace files in {}: set HERDR_SIDEBAR_CODE_WORKSPACE",
            dir.display()
        ));
    }
    Ok(None)
}

fn read_registrations(root: &Path) -> Registrations {
    let mut result = Registrations {
        by_path: HashMap::new(),
        invalid_tasks: HashMap::new(),
        issue: None,
    };
    let tasks_path = root.join("tasks");
    let tasks = match fs::read_dir(&tasks_path) {
        Ok(tasks) => tasks,
        Err(error) => {
            result.issue = Some(format!("cannot read {}: {error}", tasks_path.display()));
            return result;
        }
    };
    for task_dir in tasks {
        let task_dir = match task_dir {
            Ok(task_dir) => task_dir,
            Err(error) => {
                result.issue = Some(format!(
                    "cannot enumerate {}: {error}",
                    tasks_path.display()
                ));
                continue;
            }
        };
        let dir = task_dir.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(slug) = dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let json_path = dir.join("task.json");
        if !json_path.exists() {
            result
                .invalid_tasks
                .insert(slug.to_string(), format!("missing {}", json_path.display()));
            continue;
        }
        let task: TaskFile = match fs::read_to_string(&json_path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
        {
            Ok(task) => task,
            Err(error) => {
                result.invalid_tasks.insert(
                    slug.to_string(),
                    format!("invalid {}: {error}", json_path.display()),
                );
                continue;
            }
        };
        if task.id != slug {
            result.invalid_tasks.insert(
                slug.to_string(),
                format!("task id {} does not match {}", task.id, slug),
            );
            continue;
        }
        let Some(workspace) = task.workspace else {
            result.invalid_tasks.insert(
                slug.to_string(),
                format!("task {} has no workspace registration", slug),
            );
            continue;
        };
        let declared = workspace.repos.len();
        for (repo, registered_path) in workspace.worktrees {
            if !workspace.repos.contains(&repo) {
                result.invalid_tasks.insert(
                    slug.to_string(),
                    format!("task {} registers undeclared repository {repo}", slug),
                );
                continue;
            }
            let registered = PathBuf::from(registered_path);
            let registered = if registered.is_absolute() {
                registered
            } else {
                root.join(registered)
            };
            result
                .by_path
                .entry(normalized(&registered))
                .or_default()
                .push(Registration {
                    task: slug.to_string(),
                    repo,
                    declared,
                });
        }
    }
    result
}

fn read_registry(root: &Path) -> Result<HashMap<String, Result<Target, String>>, String> {
    let path = root.join("config/repositories.yaml");
    let text =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let registry: Registry =
        serde_yaml::from_str(&text).map_err(|e| format!("invalid {}: {e}", path.display()))?;
    let mut targets = HashMap::new();
    for entry in registry.repositories {
        let target = match (
            entry.remote.filter(|value| !value.trim().is_empty()),
            entry
                .default_branch
                .filter(|value| !value.trim().is_empty()),
        ) {
            (Some(remote_url), Some(branch)) => Ok(Target { remote_url, branch }),
            _ => Err(format!(
                "repository {} has no remote/default_branch in config/repositories.yaml",
                entry.id
            )),
        };
        match targets.entry(entry.id.clone()) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(target);
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                let _ = slot.insert(Err(format!(
                    "duplicate repository {} in config/repositories.yaml",
                    entry.id
                )));
            }
        }
    }
    Ok(targets)
}

fn normalized(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        // Missing selections still need an absolute, stable path for error display.
        let mut result = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                other => result.push(other.as_os_str()),
            }
        }
        result
    })
}

fn task_slug<'a>(path: &'a Path, root: &Path) -> Option<&'a str> {
    // The path is used only as a diagnostic hint, never to grant task identity.
    let relative = path.strip_prefix(root.join("worktrees")).ok()?;
    relative.components().next()?.as_os_str().to_str()
}

fn add_error(folder: &mut Folder, error: String) {
    match &mut folder.error {
        Some(existing) => {
            existing.push_str("; ");
            existing.push_str(&error);
        }
        None => folder.error = Some(error),
    }
}
