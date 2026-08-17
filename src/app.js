import { invoke } from '@tauri-apps/api/core';

const JSON_FILE = 'instances.json';
const DEFAULT_SCAN_DIR = '.'; // Set this to your default scanning root path

// DOM Elements
const nameInput = document.getElementById('nameInput');
const folderSelect = document.getElementById('folderSelect');
const repoSelect = document.getElementById('repoSelect');
const btnAdd = document.getElementById('btnAdd');
const instanceList = document.getElementById('instanceList');
const scanStatus = document.getElementById('scanStatus');

// State caches for dropdown selections
let scannedFolders = [];
let scannedRepos = [];

// Automatic Initialization
async function init() {
  await Promise.all([
    runAutoScan(),
    loadInstances()
  ]);
}

// Automatically scan root directory and populate dropdowns
async function runAutoScan() {
  try {
    scanStatus.textContent = 'Scanning...';
    
    const result = await invoke('scan_instances', { 
      rootDir: DEFAULT_SCAN_DIR, 
      jsonFile: JSON_FILE 
    });

    scannedFolders = result.folders || [];
    scannedRepos = result.repos || [];

    populateFolderDropdown(scannedFolders);
    populateRepoDropdown(scannedRepos);

    scanStatus.textContent = 'Scan complete';
  } catch (error) {
    console.error('Auto-scan failed:', error);
    scanStatus.textContent = 'Scan failed';
  }
}

// Populate folder select dropdown
function populateFolderDropdown(folders) {
  folderSelect.innerHTML = '';

  if (folders.length === 0) {
    folderSelect.innerHTML = '<option value="">No folders found</option>';
    return;
  }

  folders.forEach((f) => {
    const opt = document.createElement('option');
    opt.value = f.folder;
    opt.textContent = `${f.folder_name} ${f.has_pack_toml ? '(pack.toml present)' : ''}`;
    folderSelect.appendChild(opt);
  });
}

// Populate repo select dropdown
function populateRepoDropdown(repos) {
  repoSelect.innerHTML = '';

  if (repos.length === 0) {
    repoSelect.innerHTML = '<option value="">No Git repos found</option>';
    return;
  }

  // Add an empty/none option if repo selection is optional
  const defaultOpt = document.createElement('option');
  defaultOpt.value = '';
  defaultOpt.textContent = '-- Select a repository --';
  repoSelect.appendChild(defaultOpt);

  repos.forEach((r) => {
    const opt = document.createElement('option');
    opt.value = JSON.stringify({ url: r.remote_url, repo_name: r.repo_name });
    opt.textContent = `${r.repo_name} (${r.remote_url})`;
    repoSelect.appendChild(opt);
  });
}

// Fetch and display saved instances list
async function loadInstances() {
  try {
    const instances = await invoke('get_instances');
    renderInstances(instances);
  } catch (error) {
    console.error('Failed to load instances:', error);
  }
}

// Render the instance list UI
function renderInstances(instances) {
  instanceList.innerHTML = '';

  if (!instances || instances.length === 0) {
    instanceList.innerHTML = '<li class="instance-item" style="color: var(--text-muted);">No instances configured.</li>';
    return;
  }

  instances.forEach((inst) => {
    const li = document.createElement('li');
    li.className = 'instance-item';

    li.innerHTML = `
      <div class="instance-info">
        <span class="instance-name">${escapeHtml(inst.repo_name)}</span>
        <span class="instance-path">${escapeHtml(inst.folder)}</span>
        <span class="instance-url">${escapeHtml(inst.url)}</span>
      </div>
      <button class="btn-danger" data-folder="${escapeHtml(inst.folder)}">Delete</button>
    `;

    const deleteBtn = li.querySelector('.btn-danger');
    deleteBtn.addEventListener('click', () => handleDelete(inst.folder));

    instanceList.appendChild(li);
  });
}

// Handle adding an instance using user-provided Name and selected inputs
async function handleAdd() {
  const customName = nameInput.value.trim();
  const selectedFolder = folderSelect.value;
  const repoDataRaw = repoSelect.value;

  if (!customName) return alert('Please enter an Instance Name.');
  if (!selectedFolder) return alert('Please select a folder.');

  let remoteUrl = '';
  let repoName = customName;

  if (repoDataRaw) {
    try {
      const parsedRepo = JSON.parse(repoDataRaw);
      remoteUrl = parsedRepo.url;
    } catch (e) {
      console.warn('Failed to parse repo selection', e);
    }
  }

  try {
    btnAdd.disabled = true;
    btnAdd.innerText = 'Saving...';

    await invoke('add_or_update_instance', {
      folder: selectedFolder,
      url: remoteUrl,
      repoName: repoName,
      jsonFile: JSON_FILE
    });

    nameInput.value = '';
    await loadInstances();
  } catch (error) {
    alert('Failed to add instance: ' + error);
  } finally {
    btnAdd.disabled = false;
    btnAdd.innerText = 'Save & Run Packwiz';
  }
}

// Handle deleting an instance
async function handleDelete(folderPath) {
  try {
    await invoke('delete_instance', { folder: folderPath, jsonFile: JSON_FILE });
    await loadInstances();
  } catch (error) {
    alert('Failed to delete instance: ' + error);
  }
}

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// Event Listeners
btnAdd.addEventListener('click', handleAdd);

// Run on application load
init();