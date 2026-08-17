(() => {
  // Use global Tauri APIs provided when withGlobalTauri is true
  const { invoke } = window.__TAURI__.core;
  const { getCurrentWindow } = window.__TAURI__.window;

  const JSON_FILE = 'instances.json';

  // DOM Elements
  const nameInput = document.getElementById('nameInput');
  const urlInput = document.getElementById('urlInput');
  const folderSelect = document.getElementById('folderSelect');
  const btnAdd = document.getElementById('btnAdd');
  const btnRunPackwiz = document.getElementById('btnRunPackwiz');
  const instanceList = document.getElementById('instanceList');
  const scanStatus = document.getElementById('scanStatus');

  // App Initialization
  async function init() {
    await Promise.all([
      runAutoScan(),
      loadInstances()
    ]);

    // Execute packwiz for all saved instances on launch
    await runPackwizAll();

    // Ensure the main window is visible when opened manually
    try {
      const appWindow = getCurrentWindow();
      await appWindow.show();
    } catch (e) {
      console.warn('Could not show window:', e);
    }
  }

  // Scan .minecraft/versions directory
  async function runAutoScan() {
    try {
      scanStatus.textContent = 'Scanning versions...';
      
      const result = await invoke('scan_instances', { customPath: null });

      populateFolderDropdown(result.folders || []);
      scanStatus.textContent = 'Scan complete';
    } catch (error) {
      console.error('Auto-scan error:', error);
      scanStatus.textContent = 'Scan failed: ' + error;
      folderSelect.innerHTML = '<option value="">Failed to scan versions</option>';
    }
  }

  // Populate folder select dropdown
  function populateFolderDropdown(folders) {
    folderSelect.innerHTML = '';

    if (folders.length === 0) {
      folderSelect.innerHTML = '<option value="">No version folders found</option>';
      return;
    }

    folders.forEach((f) => {
      const opt = document.createElement('option');
      opt.value = f.folder;
      opt.textContent = `${f.folder_name} ${f.has_pack_toml ? '(pack.toml found)' : ''}`;
      folderSelect.appendChild(opt);
    });
  }

  // Load instances from instances.json
  async function loadInstances() {
    try {
      const instances = await invoke('get_instances');
      renderInstances(instances);
    } catch (error) {
      console.error('Failed to load instances:', error);
    }
  }

  // Render instance list
  function renderInstances(instances) {
    instanceList.innerHTML = '';

    if (!instances || instances.length === 0) {
      instanceList.innerHTML = '<li class="instance-item" style="color: #888;">No saved instances yet.</li>';
      return;
    }

    instances.forEach((inst) => {
      const li = document.createElement('li');
      li.className = 'instance-item';

      li.innerHTML = `
        <div class="instance-info">
          <strong class="instance-name">${escapeHtml(inst.repo_name)}</strong><br />
          <small class="instance-path">${escapeHtml(inst.folder)}</small><br />
          <small class="instance-url">${escapeHtml(inst.url || 'No Remote URL')}</small>
        </div>
        <button class="btn-danger">Delete</button>
      `;

      const deleteBtn = li.querySelector('.btn-danger');
      deleteBtn.addEventListener('click', () => handleDelete(inst.folder));

      instanceList.appendChild(li);
    });
  }

  // Run packwiz on all instances
  async function runPackwizAll() {
    try {
      const instances = await invoke('get_instances');
      for (const inst of instances) {
        if (inst.folder && inst.url) {
          await invoke('run_packwiz_command', {
            folder: inst.folder,
            url: inst.url,
            repoName: inst.repo_name
          });
        }
      }
    } catch (error) {
      console.error('Failed to execute packwiz on all instances:', error);
    }
  }

  // Save new instance
  async function handleAdd() {
    const repoName = nameInput.value.trim();
    const remoteUrl = urlInput.value.trim();
    const selectedFolder = folderSelect.value;

    if (!repoName) return alert('Please enter an Instance / Repo Name.');
    if (!selectedFolder) return alert('Please select a version folder.');

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
      urlInput.value = '';
      await loadInstances();
    } catch (error) {
      alert('Failed to add instance: ' + error);
    } finally {
      btnAdd.disabled = false;
      btnAdd.innerText = 'Save Instance';
    }
  }

  // Trigger manual Packwiz execution
  async function handleRunPackwizManual() {
    const selectedFolder = folderSelect.value;
    const remoteUrl = urlInput.value.trim();
    const repoName = nameInput.value.trim();

    btnRunPackwiz.disabled = true;
    btnRunPackwiz.innerText = 'Running...';

    try {
      if (!selectedFolder) {
        await runPackwizAll();
      } else {
        await invoke('run_packwiz_command', {
          folder: selectedFolder,
          url: remoteUrl,
          repoName: repoName
        });
      }
      alert('Packwiz command executed!');
    } catch (error) {
      alert('Packwiz error: ' + error);
    } finally {
      btnRunPackwiz.disabled = false;
      btnRunPackwiz.innerText = 'Run Packwiz';
    }
  }

  // Delete instance
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
  btnRunPackwiz.addEventListener('click', handleRunPackwizManual);

  // Run on startup
  init();
})();