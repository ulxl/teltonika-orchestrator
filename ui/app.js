let eventSource = null;
let currentTemplates = [];
let currentStandardPasswords = ['qwerty123', 'qwerty123!'];
let editingTemplateName = null;
let allTargetIps = [];
let executionStartTime = null;
let activeFilter = 'all';
let latestExcelFilename = '';
let autoScrollEnabled = true;
let activeLogFilter = 'all';
let logSearchQuery = '';
let storedLogs = [];
let isCancelling = false;
let currentWorkspaceDir = '';
let currentResultsDir = 'results';
let execTotal = 0;
let execSuccess = 0;
let execFailed = 0;
let pendingLogNodes = [];
let pendingResults = [];
let pendingProgress = null;
let rafId = null;

function initTheme() {
    let saved = null;
    try {
        saved = localStorage.getItem('teltonika_theme');
    } catch (_) {}
    const systemPrefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
    const isDark = saved !== null ? saved === 'dark' : Boolean(systemPrefersDark);
    applyTheme(isDark);

    if (window.matchMedia) {
        window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
            let userOverride = null;
            try {
                userOverride = localStorage.getItem('teltonika_theme');
            } catch (_) {}
            if (userOverride === null) {
                applyTheme(e.matches);
            }
        });
    }
}

function applyTheme(isDark) {
    if (isDark) {
        document.body.classList.add('dark-mode');
    } else {
        document.body.classList.remove('dark-mode');
    }
    updateThemeButton(isDark);
}

function toggleTheme() {
    const isDark = !document.body.classList.contains('dark-mode');
    const newTheme = isDark ? 'dark' : 'light';
    try {
        localStorage.setItem('teltonika_theme', newTheme);
        localStorage.setItem('teltonika_theme_pref', newTheme);
    } catch (_) {}
    applyTheme(isDark);
    const tpSelect = document.getElementById('settingsThemeSelect');
    if (tpSelect) tpSelect.value = newTheme;
    saveCurrentConfig();
}

function updateThemeButton(isDark) {
    const btn = document.getElementById('themeToggleBtn');
    if (btn) btn.setAttribute('aria-checked', isDark ? 'true' : 'false');
}

function setEngineStatus(text, state) {
    const statusText = document.getElementById('connectionStatus');
    const statusDot = document.querySelector('.status-dot');
    if (statusText) statusText.innerText = text;
    if (statusDot) {
        statusDot.className = 'status-dot' + (state ? ' ' + state : '');
    }
}

function copyMac(btn, macText) {
    if (!macText || macText === '-' || macText === 'None') return;
    navigator.clipboard.writeText(macText).then(() => {
        const orig = btn.innerText;
        btn.innerText = 'Copied!';
        btn.style.background = 'var(--badge-success-bg)';
        btn.style.color = 'var(--badge-success-text)';
        setTimeout(() => {
            btn.innerText = orig;
            btn.style.background = '';
            btn.style.color = '';
        }, 1200);
    });
}

function initSettings() {
    initTheme();
    try {
        const savedFontFamily = localStorage.getItem('teltonika_font_family') || 'geist';
        changeFontFamily(savedFontFamily, false);
        const ffSelect = document.getElementById('settingsFontFamilySelect');
        if (ffSelect) ffSelect.value = savedFontFamily;

        const savedFontSize = localStorage.getItem('teltonika_font_size') || 'normal';
        changeFontSize(savedFontSize, false);
        const fsSelect = document.getElementById('settingsFontSizeSelect');
        if (fsSelect) fsSelect.value = savedFontSize;

        const savedWorkers = localStorage.getItem('teltonika_default_workers');
        if (savedWorkers) {
            document.getElementById('workers').value = savedWorkers;
            const swInput = document.getElementById('settingsDefaultWorkers');
            if (swInput) swInput.value = savedWorkers;
        }

        const savedThemePref = localStorage.getItem('teltonika_theme_pref') || 'system';
        const tpSelect = document.getElementById('settingsThemeSelect');
        if (tpSelect) tpSelect.value = savedThemePref;

        const savedWorkspaceDir = localStorage.getItem('teltonika_workspace_dir');
        if (savedWorkspaceDir) {
            currentWorkspaceDir = savedWorkspaceDir;
            const wsInput = document.getElementById('settingsWorkspaceDir');
            if (wsInput) wsInput.value = savedWorkspaceDir;
        }

        const rPort = localStorage.getItem('teltonika_default_router_port') || '443';
        const apPort = localStorage.getItem('teltonika_default_ap_port') || '443';
        const sshPort = localStorage.getItem('teltonika_default_ssh_port') || '22';
        if (document.getElementById('settingsRouterPort')) document.getElementById('settingsRouterPort').value = rPort;
        if (document.getElementById('settingsApPort')) document.getElementById('settingsApPort').value = apPort;
        if (document.getElementById('settingsSshPort')) document.getElementById('settingsSshPort').value = sshPort;
        if (document.getElementById('port')) document.getElementById('port').value = rPort;
    } catch (_) {}
}

function openSettingsModal() {
    const modal = document.getElementById('settingsModal');
    if (modal) modal.style.display = 'flex';
    const status = document.getElementById('settingsSaveStatus');
    if (status) status.style.display = 'none';
    const stdInput = document.getElementById('settingsStandardPasswords');
    if (stdInput && currentStandardPasswords.length > 0) {
        stdInput.value = currentStandardPasswords.join(', ');
    }
}

function changeStandardPasswords(val) {
    if (val && val.trim()) {
        const parsed = val.split(',').map(s => s.trim()).filter(Boolean);
        if (parsed.length > 0) {
            currentStandardPasswords = parsed;
        }
    }
    saveCurrentConfig();
}

async function closeSettingsModal() {
    await saveCurrentConfig();
    const modal = document.getElementById('settingsModal');
    if (modal) modal.style.display = 'none';
}

async function saveSettingsModal() {
    await saveCurrentConfig();
    const status = document.getElementById('settingsSaveStatus');
    if (status) {
        status.style.display = 'inline';
        setTimeout(() => {
            if (status) status.style.display = 'none';
        }, 2500);
    }
}

function changeDefaultTheme(mode, persist = true) {
    try {
        localStorage.setItem('teltonika_theme_pref', mode);
        if (mode === 'system') {
            localStorage.removeItem('teltonika_theme');
            const isDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
            applyTheme(Boolean(isDark));
        } else if (mode === 'dark') {
            localStorage.setItem('teltonika_theme', 'dark');
            applyTheme(true);
        } else if (mode === 'light') {
            localStorage.setItem('teltonika_theme', 'light');
            applyTheme(false);
        }
    } catch (_) {}
    const tpSelect = document.getElementById('settingsThemeSelect');
    if (tpSelect) tpSelect.value = mode;
    if (persist) {
        saveCurrentConfig();
    }
}

function changeFontFamily(familyKey, persist = true) {
    document.body.classList.remove('font-family-geist', 'font-family-inter', 'font-family-jakarta', 'font-family-system');
    document.body.classList.add(`font-family-${familyKey}`);
    const ffSelect = document.getElementById('settingsFontFamilySelect');
    if (ffSelect) ffSelect.value = familyKey;
    try {
        localStorage.setItem('teltonika_font_family', familyKey);
    } catch (_) {}
    if (persist) {
        saveCurrentConfig();
    }
}

function changeFontSize(sizeKey, persist = true) {
    document.body.classList.remove('font-compact', 'font-normal', 'font-large');
    document.body.classList.add(`font-${sizeKey}`);
    document.body.style.zoom = '';
    const fsSelect = document.getElementById('settingsFontSizeSelect');
    if (fsSelect) fsSelect.value = sizeKey;
    try {
        localStorage.setItem('teltonika_font_size', sizeKey);
    } catch (_) {}
    if (persist) {
        saveCurrentConfig();
    }
}

function changeDefaultWorkers(val, persist = true) {
    const num = Math.max(1, Math.min(50, parseInt(val) || 5));
    document.getElementById('workers').value = num;
    const swInput = document.getElementById('settingsDefaultWorkers');
    if (swInput) swInput.value = num;
    try {
        localStorage.setItem('teltonika_default_workers', num);
    } catch (_) {}
    if (persist) {
        saveCurrentConfig();
    }
}

function getSavedRouterPort() {
    const el = document.getElementById('settingsRouterPort');
    if (el && el.value) return el.value;
    try {
        const p = localStorage.getItem('teltonika_default_router_port');
        if (p) return p;
    } catch (_) {}
    return '443';
}

function getSavedApPort() {
    const el = document.getElementById('settingsApPort');
    if (el && el.value) return el.value;
    try {
        const p = localStorage.getItem('teltonika_default_ap_port');
        if (p) return p;
    } catch (_) {}
    return '443';
}

function getSavedSshPort() {
    const el = document.getElementById('settingsSshPort');
    if (el && el.value) return el.value;
    try {
        const p = localStorage.getItem('teltonika_default_ssh_port');
        if (p) return p;
    } catch (_) {}
    return '22';
}

function changeDefaultPorts() {
    const rPort = getSavedRouterPort();
    const apPort = getSavedApPort();
    const sshPort = getSavedSshPort();
    try {
        localStorage.setItem('teltonika_default_router_port', rPort);
        localStorage.setItem('teltonika_default_ap_port', apPort);
        localStorage.setItem('teltonika_default_ssh_port', sshPort);
    } catch (_) {}
    const isAP = document.getElementById('tabAP').classList.contains('active');
    const portInput = document.getElementById('port');
    if (portInput) {
        portInput.value = isAP ? apPort : rPort;
    }
    saveCurrentConfig();
}

async function saveCurrentConfig() {
    const themeSelect = document.getElementById('settingsThemeSelect');
    const theme = themeSelect ? themeSelect.value : (document.body.classList.contains('dark-mode') ? 'dark' : 'light');
    const fontFamilySelect = document.getElementById('settingsFontFamilySelect');
    const fontFamily = fontFamilySelect ? fontFamilySelect.value : 'geist';
    const fontSizeSelect = document.getElementById('settingsFontSizeSelect');
    const fontSize = fontSizeSelect ? fontSizeSelect.value : 'normal';
    const workersInput = document.getElementById('settingsDefaultWorkers');
    const workers = workersInput ? (parseInt(workersInput.value) || 5) : 5;
    const wsDirInput = document.getElementById('settingsWorkspaceDir');
    const wsDir = wsDirInput ? (wsDirInput.value.trim() || currentWorkspaceDir) : currentWorkspaceDir;
    currentWorkspaceDir = wsDir;

    const routerPort = parseInt(getSavedRouterPort()) || 443;
    const apPort = parseInt(getSavedApPort()) || 443;
    const sshPort = parseInt(getSavedSshPort()) || 22;

    try {
        localStorage.setItem('teltonika_theme_pref', theme);
        localStorage.setItem('teltonika_font_family', fontFamily);
        localStorage.setItem('teltonika_font_size', fontSize);
        localStorage.setItem('teltonika_default_workers', workers);
        localStorage.setItem('teltonika_workspace_dir', wsDir);
        localStorage.setItem('teltonika_default_router_port', routerPort);
        localStorage.setItem('teltonika_default_ap_port', apPort);
        localStorage.setItem('teltonika_default_ssh_port', sshPort);
    } catch (_) {}

    const stdInput = document.getElementById('settingsStandardPasswords');
    let stdPwds = currentStandardPasswords;
    if (stdInput && stdInput.value.trim()) {
        const parsed = stdInput.value.split(',').map(s => s.trim()).filter(Boolean);
        if (parsed.length > 0) {
            stdPwds = parsed;
            currentStandardPasswords = parsed;
        }
    }

    try {
        await fetch('/api/templates', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                templates: currentTemplates,
                active_template: document.getElementById('passwordTemplateSelect') ? document.getElementById('passwordTemplateSelect').value : '',
                standard_passwords: stdPwds,
                workspace_dir: wsDir,
                theme: theme,
                font_family: fontFamily,
                font_size: fontSize,
                default_workers: workers,
                default_router_port: routerPort,
                default_ap_port: apPort,
                default_ssh_port: sshPort
            })
        });
    } catch (_) {}
}

document.addEventListener('DOMContentLoaded', () => {
    initSettings();
    loadTemplates();
    updateIpStats();
    switchCategory('general');
    initCopyListener();
    initTableResizing();
    setTimeout(() => checkForAppUpdate(true), 2500);
});

function initCopyListener() {
    document.getElementById('resultsBody').addEventListener('click', (e) => {
        const td = e.target.closest('td.copyable');
        if (!td) return;
        const text = td.innerText.trim();
        if (!text || text === '-' || text === 'N/A') return;
        navigator.clipboard.writeText(text).then(() => {
            const original = td.innerText;
            td.innerText = 'Copied!';
            td.style.color = 'var(--success)';
            setTimeout(() => {
                td.innerText = original;
                td.style.color = '';
            }, 800);
        });
    });
}

function copySsids(btn, ...pairs) {
    let lines = [];
    let count = 1;
    for (let i = 0; i < pairs.length; i += 2) {
        const s = pairs[i];
        const p = pairs[i + 1];
        if (s && s !== '-' && s !== 'N/A') {
            lines.push(`SSID${count}: ${s} Pass: ${p || 'N/A'}`);
            count++;
        }
    }
    if (lines.length === 0) return;
    const fullText = lines.join('\n');
    navigator.clipboard.writeText(fullText).then(() => {
        const orig = btn.innerText;
        btn.innerText = 'Copied!';
        btn.style.background = 'var(--success)';
        btn.style.color = '#000';
        setTimeout(() => {
            btn.innerText = orig;
            btn.style.background = '';
            btn.style.color = '';
        }, 1200);
    });
}

function switchCategory(cat) {
    document.getElementById('tabGeneral').classList.toggle('active', cat === 'general');
    document.getElementById('tabRouter').classList.toggle('active', cat === 'router');
    document.getElementById('tabAP').classList.toggle('active', cat === 'ap');

    document.querySelectorAll('.opt-general').forEach(el => el.style.display = cat === 'general' ? 'block' : 'none');
    document.querySelectorAll('.opt-router').forEach(el => el.style.display = cat === 'router' ? 'block' : 'none');
    document.querySelectorAll('.opt-ap').forEach(el => el.style.display = cat === 'ap' ? 'block' : 'none');

    const select = document.getElementById('action');
    const portInput = document.getElementById('port');

    if (cat === 'general') {
        select.value = 'verify';
        portInput.value = getSavedRouterPort();
        portInput.readOnly = false;
        portInput.style.opacity = '1';
    } else if (cat === 'router') {
        select.value = 'extract_data';
        portInput.value = getSavedRouterPort();
        portInput.readOnly = false;
        portInput.style.opacity = '1';
    } else if (cat === 'ap') {
        select.value = 'check_wifi';
        portInput.value = getSavedApPort();
        portInput.readOnly = false;
        portInput.style.opacity = '1';
    }

    document.getElementById('deviceType').value = cat === 'ap' ? 'ap' : 'router';
    onActionChange();
}

function onActionChange() {
    const action = document.getElementById('action').value;
    const isAP = action.includes('wifi') || action.includes('mac') || action === 'extract_data_ap';
    const isFirmware = action === 'update_firmware';
    const isMac = action === 'add_mac';
    const isBundle = action.includes('bundle');

    const userInput = document.getElementById('username');
    const portInput = document.getElementById('port');

    if (isFirmware) {
        userInput.value = 'root';
        portInput.value = getSavedSshPort();
        portInput.readOnly = false;
        portInput.style.opacity = '1';
    } else {
        const cat = document.getElementById('tabAP').classList.contains('active') ? 'ap' : (document.getElementById('tabRouter').classList.contains('active') ? 'router' : 'general');
        if (cat === 'ap') {
            userInput.value = 'admin';
            if (portInput.value === getSavedSshPort() || portInput.value === '822' || portInput.value === '22') {
                portInput.value = getSavedApPort();
            }
            portInput.readOnly = false;
            portInput.style.opacity = '1';
        } else {
            if (userInput.value === 'root') userInput.value = 'admin';
            if (portInput.value === getSavedSshPort() || portInput.value === '822' || portInput.value === '22') {
                portInput.value = getSavedRouterPort();
            }
            portInput.readOnly = false;
            portInput.style.opacity = '1';
        }
    }

    const isRestore = action === 'restore_config';

    document.getElementById('firmwareDropzone').style.display = isFirmware ? 'block' : 'none';
    const backupDropzone = document.getElementById('backupDropzone');
    if (backupDropzone) backupDropzone.style.display = isRestore ? 'block' : 'none';
    document.getElementById('macGroup').style.display = isMac ? 'flex' : 'none';
    document.getElementById('portGroup').style.display = isBundle ? 'none' : 'flex';

    const header = document.getElementById('resultsHeader');
    if (action === 'check_wifi') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>MAC Whitelist</th><th>SSID & Keys</th><th style="width: 170px;">Action</th>';
    } else if (action === 'add_mac') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Added MACs</th><th>Details</th><th style="width: 110px;">Action</th>';
    } else if (action === 'extract_data') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Firmware</th><th>Serial</th><th>MAC</th><th>IMEI</th><th>ICCID</th><th>IMSI</th><th>Operator</th><th>RSSI</th><th>RSRP</th><th>RSRQ</th><th>SINR</th><th style="width: 140px;">Action</th>';
    } else if (action === 'extract_data_ap') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Firmware</th><th>Serial</th><th>MAC</th><th>SSIDs</th><th style="width: 110px;">Action</th>';
    } else if (action === 'bundle_check') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP:Port</th><th style="width: 90px;">Status</th><th>Serial</th><th>MAC</th><th>Password</th>';
    } else if (action === 'reconnect_rms_bundle') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP:Port</th><th style="width: 90px;">Status</th><th>RMS State</th>';
    } else if (action === 'reconnect_rms') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>RMS State</th>';
    } else if (action === 'update_firmware') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Password</th><th>Details</th>';
    } else if (action === 'standardize') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Old Password</th><th>New Password</th><th>Details</th>';
    } else if (action === 'backup_config') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Backup File</th><th>Subfolder</th><th>Size</th>';
    } else if (action === 'restore_config') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Details</th>';
    } else if (action === 'verify') {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Password</th><th>Details</th>';
    } else {
        header.innerHTML = '<th style="width: 40px;">#</th><th>IP Address</th><th style="width: 90px;">Status</th><th>Details</th>';
    }

    const tbody = document.getElementById('resultsBody');
    if (tbody && (tbody.children.length === 0 || tbody.querySelector('td[colspan]'))) {
        const colCount = header.querySelectorAll('th').length;
        tbody.innerHTML = `<tr><td colspan="${colCount}" style="text-align: center; color: var(--text-secondary); padding: 48px;">Ready. Configure settings and click Execute Sequence.</td></tr>`;
    }
    const headerEl = document.getElementById('resultsHeader');
    const tableEl = document.getElementById('resultsTable') || (headerEl ? headerEl.closest('table') : null);
    if (tableEl) tableEl.style.width = '100%';
    initTableResizing();
}

function initTableResizing() {
    const header = document.getElementById('resultsHeader');
    const table = document.getElementById('resultsTable') || (header ? header.closest('table') : null);
    if (!table || !header) return;
    const ths = header.querySelectorAll('th');
    if (ths.length === 0) return;

    try {
        for (let i = 0; i < localStorage.length; i++) {
            const k = localStorage.key(i);
            if (k && k.startsWith('teltonika_col_widths_')) {
                const val = JSON.parse(localStorage.getItem(k) || '[]');
                if (Array.isArray(val)) {
                    const total = val.reduce((a, b) => a + b, 0);
                    if (total < 350 || val.some(w => w < 12)) {
                        localStorage.removeItem(k);
                    }
                }
            }
        }
    } catch (_) {}

    const action = document.getElementById('action').value;
    let savedWidths = null;
    try {
        const raw = localStorage.getItem('teltonika_col_widths_' + action);
        if (raw) {
            const parsed = JSON.parse(raw);
            if (Array.isArray(parsed) && parsed.length === ths.length) {
                const total = parsed.reduce((a, b) => a + b, 0);
                if (total >= 350 && parsed.every(w => w >= 12)) {
                    savedWidths = parsed;
                } else {
                    localStorage.removeItem('teltonika_col_widths_' + action);
                }
            } else {
                localStorage.removeItem('teltonika_col_widths_' + action);
            }
        }
    } catch (_) {
        savedWidths = null;
    }

    if (savedWidths) {
        ths.forEach((th, idx) => {
            th.style.width = savedWidths[idx] + 'px';
        });
        const total = savedWidths.reduce((a, b) => a + b, 0);
        const containerW = table.parentElement ? table.parentElement.clientWidth : 0;
        table.style.width = Math.max(containerW, total) + 'px';
    } else {
        table.style.width = '100%';
        ths.forEach(th => {
            if (!th.getAttribute('style') || !th.getAttribute('style').includes('width')) {
                th.style.width = '';
            }
        });
    }

    ths.forEach((th, idx) => {
        let span = th.querySelector('.th-text');
        if (!span) {
            const rawTitle = th.innerText.trim();
            if (!th.getAttribute('title')) {
                th.setAttribute('title', rawTitle);
            }
            const textNodes = Array.from(th.childNodes).filter(n => n.nodeType === Node.TEXT_NODE);
            const text = textNodes.map(n => n.textContent).join('').trim();
            textNodes.forEach(n => n.remove());
            span = document.createElement('span');
            span.className = 'th-text';
            span.textContent = text || rawTitle;
            th.insertBefore(span, th.firstChild);
        } else if (!th.getAttribute('title')) {
            th.setAttribute('title', span.innerText.trim());
        }

        const oldResizer = th.querySelector('.col-resizer');
        if (oldResizer) oldResizer.remove();

        if (idx === ths.length - 1) return;

        const resizer = document.createElement('div');
        resizer.className = 'col-resizer';
        th.appendChild(resizer);

        resizer.addEventListener('mousedown', (e) => {
            e.preventDefault();
            e.stopPropagation();

            const startX = e.pageX;
            const containerW = table.parentElement ? table.parentElement.clientWidth : 0;

            const initialWidths = Array.from(ths).map(t => Math.round(t.getBoundingClientRect().width));
            ths.forEach((t, i) => {
                t.style.width = initialWidths[i] + 'px';
            });

            const initialTableWidth = initialWidths.reduce((a, b) => a + b, 0);
            table.style.width = initialTableWidth + 'px';

            const startColWidth = initialWidths[idx];
            const lastIdx = ths.length - 1;
            const startLastWidth = initialWidths[lastIdx];
            const sumOthers = initialTableWidth - startLastWidth - startColWidth;

            resizer.classList.add('resizing');
            document.body.style.cursor = 'col-resize';
            document.body.style.userSelect = 'none';

            function onMouseMove(moveEvent) {
                const diff = moveEvent.pageX - startX;
                const newColWidth = Math.max(12, Math.round(startColWidth + diff));
                const delta = newColWidth - startColWidth;

                th.style.width = newColWidth + 'px';

                const newTableWidth = initialTableWidth + delta;
                if (newTableWidth >= containerW) {
                    table.style.width = newTableWidth + 'px';
                    ths[lastIdx].style.width = startLastWidth + 'px';
                } else {
                    table.style.width = '100%';
                    const lastColWidth = Math.max(20, containerW - sumOthers - newColWidth);
                    ths[lastIdx].style.width = lastColWidth + 'px';
                }
            }

            function onMouseUp() {
                resizer.classList.remove('resizing');
                document.body.style.cursor = '';
                document.body.style.userSelect = '';
                document.removeEventListener('mousemove', onMouseMove);
                document.removeEventListener('mouseup', onMouseUp);

                try {
                    const finalWidths = Array.from(ths).map(t => Math.max(12, Math.round(t.getBoundingClientRect().width)));
                    localStorage.setItem('teltonika_col_widths_' + action, JSON.stringify(finalWidths));
                } catch (_) {}
            }

            document.addEventListener('mousemove', onMouseMove);
            document.addEventListener('mouseup', onMouseUp);
        });
    });
}

function getCleanIpList() {
    const raw = document.getElementById('ips').value;
    const lines = raw.split(/[\n,;]+/);
    const result = [];
    for (let line of lines) {
        const item = line.trim();
        if (item) {
            result.push(item);
        }
    }
    return result;
}

function updateIpStats() {
    const ips = getCleanIpList();
    document.getElementById('ipCounter').innerText = `${ips.length} valid`;
}

async function pasteAndCleanIps() {
    try {
        const text = await navigator.clipboard.readText();
        if (text) {
            const lines = text.split(/[\n,; \t]+/);
            const cleaned = [];
            for (let line of lines) {
                const item = line.trim();
                if (item) {
                    cleaned.push(item);
                }
            }
            if (cleaned.length > 0) {
                document.getElementById('ips').value = cleaned.join('\n');
                updateIpStats();
                return;
            }
        }
    } catch (_) {}
    const existing = getCleanIpList();
    document.getElementById('ips').value = existing.join('\n');
    updateIpStats();
}

async function loadTemplates() {
    try {
        const res = await fetch('/api/templates');
        if (res.ok) {
            const data = await res.json();
            currentTemplates = data.templates || [];
            if (data.workspace_dir) {
                currentWorkspaceDir = data.workspace_dir;
                const wsInput = document.getElementById('settingsWorkspaceDir');
                if (wsInput) wsInput.value = currentWorkspaceDir;
                try {
                    localStorage.setItem('teltonika_workspace_dir', currentWorkspaceDir);
                } catch (_) {}
            }
            if (data.results_dir) {
                currentResultsDir = data.results_dir;
                try {
                    localStorage.setItem('teltonika_results_dir', currentResultsDir);
                } catch (_) {}
            }
            if (data.theme) {
                changeDefaultTheme(data.theme, false);
            }
            if (data.font_family) {
                changeFontFamily(data.font_family, false);
            }
            if (data.font_size) {
                changeFontSize(data.font_size, false);
            }
            if (data.default_workers) {
                document.getElementById('workers').value = data.default_workers;
                const swInput = document.getElementById('settingsDefaultWorkers');
                if (swInput) swInput.value = data.default_workers;
                try {
                    localStorage.setItem('teltonika_default_workers', data.default_workers);
                } catch (_) {}
            }
            if (data.default_router_port) {
                const el = document.getElementById('settingsRouterPort');
                if (el) el.value = data.default_router_port;
                try { localStorage.setItem('teltonika_default_router_port', data.default_router_port); } catch (_) {}
            }
            if (data.default_ap_port) {
                const el = document.getElementById('settingsApPort');
                if (el) el.value = data.default_ap_port;
                try { localStorage.setItem('teltonika_default_ap_port', data.default_ap_port); } catch (_) {}
            }
            if (data.default_ssh_port) {
                const el = document.getElementById('settingsSshPort');
                if (el) el.value = data.default_ssh_port;
                try { localStorage.setItem('teltonika_default_ssh_port', data.default_ssh_port); } catch (_) {}
            }
            if (data.standard_passwords && Array.isArray(data.standard_passwords) && data.standard_passwords.length > 0) {
                currentStandardPasswords = data.standard_passwords;
                const stdInput = document.getElementById('settingsStandardPasswords');
                if (stdInput) stdInput.value = currentStandardPasswords.join(', ');
            }
            const isAP = document.getElementById('tabAP').classList.contains('active');
            const portInput = document.getElementById('port');
            if (portInput) {
                portInput.value = isAP ? getSavedApPort() : getSavedRouterPort();
            }
            renderTemplateSelect(data.active_template);
            if (data.is_first_run) {
                const wsInput = document.getElementById('firstRunWorkspaceDir');
                if (wsInput) wsInput.value = data.suggested_workspace_dir || '';
                const modal = document.getElementById('firstRunModal');
                if (modal) modal.style.display = 'flex';
            }
        }
    } catch (_) {}
}

function renderTemplateSelect(activeName) {
    const sel = document.getElementById('passwordTemplateSelect');
    if (!sel) return;
    sel.innerHTML = '';
    for (let t of currentTemplates) {
        const opt = document.createElement('option');
        opt.value = t.name;
        opt.innerText = t.name;
        sel.appendChild(opt);
    }
    if (activeName && currentTemplates.some(t => t.name === activeName)) {
        sel.value = activeName;
    } else if (currentTemplates.length > 0) {
        sel.value = currentTemplates[0].name;
    }
    onTemplateSelect();
}

function onTemplateSelect() {
    const sel = document.getElementById('passwordTemplateSelect');
    const pwdInput = document.getElementById('passwords');
    if (!sel || !pwdInput) return;
    const chosen = currentTemplates.find(t => t.name === sel.value);
    if (chosen) {
        pwdInput.value = chosen.passwords.join(', ');
    } else if (currentTemplates.length > 0) {
        sel.value = currentTemplates[0].name;
        pwdInput.value = currentTemplates[0].passwords.join(', ');
    } else {
        pwdInput.value = '';
    }
}

function openTemplateModal() {
    const list = document.getElementById('templateList');
    if (!list) return;
    list.innerHTML = '';
    for (let t of currentTemplates) {
        const item = document.createElement('div');
        item.className = 'template-item';

        const info = document.createElement('div');
        info.className = 'template-item-info';

        const nameSpan = document.createElement('span');
        nameSpan.className = 'template-item-name';
        nameSpan.textContent = t.name;

        const pwdsSpan = document.createElement('span');
        pwdsSpan.className = 'template-item-pwds';
        pwdsSpan.textContent = t.passwords.join(', ');

        info.appendChild(nameSpan);
        info.appendChild(pwdsSpan);

        const actions = document.createElement('div');
        actions.style.display = 'flex';
        actions.style.gap = '6px';
        actions.style.alignItems = 'center';

        const editBtn = document.createElement('button');
        editBtn.type = 'button';
        editBtn.className = 'btn btn-secondary btn-sm';
        editBtn.textContent = 'Edit';
        editBtn.onclick = () => editTemplate(t.name);

        const delBtn = document.createElement('button');
        delBtn.type = 'button';
        delBtn.className = 'btn btn-danger btn-sm';
        delBtn.textContent = 'Delete';
        delBtn.onclick = () => deleteTemplate(t.name);

        actions.appendChild(editBtn);
        actions.appendChild(delBtn);

        item.appendChild(info);
        item.appendChild(actions);
        list.appendChild(item);
    }
    document.getElementById('templateModal').style.display = 'flex';
}

function closeTemplateModal() {
    cancelTemplateEdit();
    document.getElementById('templateModal').style.display = 'none';
}

function editTemplate(name) {
    const t = currentTemplates.find(item => item.name === name);
    if (!t) return;
    editingTemplateName = t.name;
    document.getElementById('newTemplateName').value = t.name;
    document.getElementById('newTemplatePasswords').value = t.passwords.join(', ');
    const title = document.getElementById('templateFormTitle');
    if (title) title.textContent = `Edit Template: ${t.name}`;
    const submitBtn = document.getElementById('templateSubmitBtn');
    if (submitBtn) submitBtn.textContent = 'Save';
    const cancelBtn = document.getElementById('cancelEditTemplateBtn');
    if (cancelBtn) cancelBtn.style.display = 'inline-block';
    const pwInput = document.getElementById('newTemplatePasswords');
    if (pwInput) pwInput.focus();
}

function cancelTemplateEdit() {
    editingTemplateName = null;
    const nameInput = document.getElementById('newTemplateName');
    if (nameInput) nameInput.value = '';
    const pwInput = document.getElementById('newTemplatePasswords');
    if (pwInput) pwInput.value = '';
    const title = document.getElementById('templateFormTitle');
    if (title) title.textContent = 'Add New Template';
    const submitBtn = document.getElementById('templateSubmitBtn');
    if (submitBtn) submitBtn.textContent = 'Add';
    const cancelBtn = document.getElementById('cancelEditTemplateBtn');
    if (cancelBtn) cancelBtn.style.display = 'none';
}

async function saveOrUpdateTemplate() {
    const name = document.getElementById('newTemplateName').value.trim();
    const pwRaw = document.getElementById('newTemplatePasswords').value.trim();
    if (!name || !pwRaw) return;
    const passwords = pwRaw.split(',').map(p => p.trim()).filter(Boolean);
    if (passwords.length === 0) return;

    if (editingTemplateName) {
        const idx = currentTemplates.findIndex(t => t.name === editingTemplateName);
        if (idx !== -1) {
            currentTemplates[idx] = { name, passwords };
        } else {
            currentTemplates.push({ name, passwords });
        }
        cancelTemplateEdit();
    } else {
        const existing = currentTemplates.find(t => t.name === name);
        if (existing) {
            existing.passwords = passwords;
        } else {
            currentTemplates.push({ name, passwords });
        }
        document.getElementById('newTemplateName').value = '';
        document.getElementById('newTemplatePasswords').value = '';
    }

    await saveTemplates();
    openTemplateModal();
    renderTemplateSelect(name);
}

async function addNewTemplate() {
    await saveOrUpdateTemplate();
}

async function deleteTemplate(name) {
    if (editingTemplateName === name) {
        cancelTemplateEdit();
    }
    currentTemplates = currentTemplates.filter(t => t.name !== name);
    await saveTemplates();
    openTemplateModal();
    const nextActive = currentTemplates.length > 0 ? currentTemplates[0].name : '';
    renderTemplateSelect(nextActive);
}

async function changeWorkspaceDir(val, persist = true) {
    const cleaned = (val || '').trim();
    currentWorkspaceDir = cleaned;
    const wsInput = document.getElementById('settingsWorkspaceDir');
    if (wsInput) wsInput.value = cleaned;
    try {
        localStorage.setItem('teltonika_workspace_dir', cleaned);
    } catch (_) {}
    if (persist) {
        await saveCurrentConfig();
    }
}

async function openWorkspaceFolder() {
    try {
        await fetch('/api/open-workspace-dir', { method: 'POST' });
    } catch (_) {}
}

async function openResultsFolder() {
    try {
        await fetch('/api/open-results-dir', { method: 'POST' });
    } catch (_) {}
}

async function openTemplatesFolder() {
    try {
        await fetch('/api/open-templates-dir', { method: 'POST' });
    } catch (_) {}
}

async function openExternalUrl(event, url) {
    if (event) {
        event.preventDefault();
        event.stopPropagation();
    }
    try {
        const res = await fetch('/api/open-url', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ url: url })
        });
        if (!res.ok) {
            window.open(url, '_blank');
        }
    } catch (_) {
        window.open(url, '_blank');
    }
}

async function checkForAppUpdate(silent = false) {
    const btn = document.getElementById('btnCheckUpdate');
    const banner = document.getElementById('updateStatusBanner');
    const bannerText = document.getElementById('updateBannerText');
    const notesText = document.getElementById('updateNotesText');
    const installBtn = document.getElementById('btnInstallUpdate');

    if (btn) {
        btn.disabled = true;
        btn.innerText = 'Checking...';
    }

    try {
        const res = await fetch('/api/check-update');
        if (!res.ok) {
            if (!silent) {
                if (banner) {
                    banner.style.display = 'flex';
                    banner.style.background = 'rgba(239, 68, 68, 0.08)';
                    banner.style.borderColor = 'rgba(239, 68, 68, 0.25)';
                }
                if (bannerText) {
                    bannerText.style.color = '#ef4444';
                    bannerText.innerText = 'Failed to check updates (HTTP ' + res.status + ')';
                }
                if (installBtn) installBtn.style.display = 'none';
                if (notesText) notesText.innerText = '';
            }
            return;
        }

        const data = await res.json();
        if (data.update_available) {
            if (banner) {
                banner.style.display = 'flex';
                banner.style.background = 'rgba(59, 130, 246, 0.08)';
                banner.style.borderColor = 'rgba(59, 130, 246, 0.25)';
            }
            if (bannerText) {
                bannerText.style.color = 'var(--accent-primary)';
                bannerText.innerText = 'Update Available: ' + data.latest_version;
            }
            if (notesText) {
                notesText.innerText = data.release_notes || 'No release notes provided.';
            }
            if (installBtn) {
                installBtn.style.display = 'inline-block';
                installBtn.disabled = false;
                installBtn.innerText = 'Update to ' + data.latest_version;
            }
        } else {
            if (!silent) {
                if (banner) {
                    banner.style.display = 'flex';
                    banner.style.background = 'rgba(34, 197, 94, 0.08)';
                    banner.style.borderColor = 'rgba(34, 197, 94, 0.25)';
                }
                if (bannerText) {
                    bannerText.style.color = '#16a34a';
                    bannerText.innerText = 'You are on the latest version (' + data.current_version + ')';
                }
                if (installBtn) installBtn.style.display = 'none';
                if (notesText) notesText.innerText = '';
            }
        }
    } catch (e) {
        if (!silent) {
            if (banner) {
                banner.style.display = 'flex';
                banner.style.background = 'rgba(239, 68, 68, 0.08)';
                banner.style.borderColor = 'rgba(239, 68, 68, 0.25)';
            }
            if (bannerText) {
                bannerText.style.color = '#ef4444';
                bannerText.innerText = 'Error checking for updates: ' + e.message;
            }
            if (installBtn) installBtn.style.display = 'none';
            if (notesText) notesText.innerText = '';
        }
    } finally {
        if (btn) {
            btn.disabled = false;
            btn.innerText = 'Check for Updates';
        }
    }
}

async function installAppUpdate() {
    const btn = document.getElementById('btnInstallUpdate');
    if (btn) {
        btn.disabled = true;
        btn.innerText = 'Updating...';
    }
    const progContainer = document.getElementById('updateProgressContainer');
    if (progContainer) progContainer.style.display = 'flex';

    startLogStream();

    try {
        const res = await fetch('/api/apply-update', { method: 'POST' });
        if (!res.ok) {
            const err = await res.json().catch(() => ({}));
            alert('Failed to start update: ' + (err.message || res.statusText));
            if (btn) {
                btn.disabled = false;
                btn.innerText = 'Update Now';
            }
        }
    } catch (e) {
        alert('Failed to start update: ' + e.message);
        if (btn) {
            btn.disabled = false;
            btn.innerText = 'Update Now';
        }
    }
}

async function browseFolderFor(targetInputId) {
    try {
        const res = await fetch('/api/browse-folder', { method: 'POST' });
        if (res.ok) {
            const data = await res.json();
            if (data && data.path) {
                const el = document.getElementById(targetInputId);
                if (el) el.value = data.path;
            }
        }
    } catch (_) {}
}

async function submitFirstRunSetup() {
    const wsDir = document.getElementById('firstRunWorkspaceDir').value.trim();
    if (!wsDir) {
        alert('Please specify a Workspace folder.');
        return;
    }
    try {
        const res = await fetch('/api/setup', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ workspace_dir: wsDir })
        });
        if (res.ok) {
            document.getElementById('firstRunModal').style.display = 'none';
            await loadTemplates();
        } else {
            alert('Failed to initialize workspace directory.');
        }
    } catch (e) {
        alert('Error connecting to engine: ' + e);
    }
}

async function saveTemplates() {
    await saveCurrentConfig();
}

function onFirmwareFileSelected(input) {
    if (input.files && input.files[0]) {
        const file = input.files[0];
        document.getElementById('firmwareFileLabel').innerHTML = `Selected: <strong>${file.name}</strong> (${(file.size / 1024 / 1024).toFixed(2)} MB)`;
    }
}

function onBackupFileSelected(input) {
    if (input.files && input.files[0]) {
        const file = input.files[0];
        document.getElementById('backupFileLabel').innerHTML = `Selected: <strong>${file.name}</strong> (${(file.size / 1024).toFixed(1)} KB)`;
    }
}

function switchOutputTab(tab) {
    document.getElementById('tabResultsBtn').classList.toggle('active', tab === 'results');
    document.getElementById('tabLogsBtn').classList.toggle('active', tab === 'logs');
    document.getElementById('resultsContainer').style.display = tab === 'results' ? 'block' : 'none';
    document.getElementById('logsContainer').style.display = tab === 'logs' ? 'block' : 'none';
    const toolbar = document.getElementById('consoleToolbar');
    if (toolbar) toolbar.style.display = tab === 'logs' ? 'flex' : 'none';
}

function filterResults(filter) {
    activeFilter = filter;
    document.getElementById('cardTotal').classList.toggle('active-filter', filter === 'all');
    document.getElementById('cardSuccess').classList.toggle('active-filter', filter === 'success');
    document.getElementById('cardFailed').classList.toggle('active-filter', filter === 'failed');

    const rows = document.querySelectorAll('#resultsBody tr[data-ip]');
    rows.forEach(row => {
        const isSuccess = row.getAttribute('data-status') === 'Success';
        const isFailed = row.getAttribute('data-status') === 'Failed';
        if (filter === 'all') row.style.display = '';
        else if (filter === 'success') row.style.display = isSuccess ? '' : 'none';
        else if (filter === 'failed') row.style.display = isFailed ? '' : 'none';
    });
}

function updateSummaryMetrics() {
    document.getElementById('valTotal').innerText = execTotal;
    document.getElementById('valSuccess').innerText = execSuccess;
    document.getElementById('valFailed').innerText = execFailed;

    if (executionStartTime) {
        const duration = ((Date.now() - executionStartTime) / 1000).toFixed(1);
        document.getElementById('valDuration').innerText = `${duration}s`;
    }
}

function prepareTableRows(ips) {
    const action = document.getElementById('action').value;
    const isBundle = action === 'bundle_check' || action === 'reconnect_rms_bundle';
    const tbody = document.getElementById('resultsBody');
    tbody.innerHTML = '';

    let targetItems = [];
    if (isBundle) {
        ips.forEach(ip => {
            const clean = ip.split(':')[0];
            [6443, 8443, 9443].forEach(p => {
                targetItems.push(`${clean}:${p}`);
            });
        });
    } else {
        targetItems = [...ips];
    }

    allTargetIps = [...targetItems];
    execTotal = targetItems.length;
    execSuccess = 0;
    execFailed = 0;

    const header = document.getElementById('resultsHeader');
    const colCount = header ? header.querySelectorAll('th').length : 4;
    const extraCols = Math.max(1, colCount - 3);

    const frag = document.createDocumentFragment();
    targetItems.forEach((item, idx) => {
        const tr = document.createElement('tr');
        tr.id = `row_${idx}_${item.replace(/[^a-zA-Z0-9_-]/g, '_')}`;
        tr.setAttribute('data-ip', item);
        const itemPort = item.includes(':') ? item.split(':')[1] : (document.getElementById('port')?.value || getSavedRouterPort());
        tr.setAttribute('data-port', itemPort);
        tr.setAttribute('data-status', 'Pending');
        let dashes = '';
        for (let c = 0; c < extraCols; c++) {
            dashes += '<td style="color: var(--text-secondary);">-</td>';
        }
        tr.innerHTML = `
            <td style="color: var(--text-secondary);">${idx + 1}</td>
            <td class="copyable">${item}</td>
            <td><span class="badge badge-pending">Pending</span></td>
            ${dashes}
        `;
        frag.appendChild(tr);
    });
    tbody.appendChild(frag);

    updateSummaryMetrics();
}

async function executeSequence(ipsToRun = null) {
    const isRetry = Boolean(ipsToRun);
    const ips = isRetry ? ipsToRun : getCleanIpList();
    if (!ips || ips.length === 0) {
        alert('Please enter at least one IP address.');
        return;
    }

    const action = document.getElementById('action').value;
    const fileInput = document.getElementById('firmwareFileInput');
    if (action === 'update_firmware' && (!fileInput.files || !fileInput.files[0])) {
        alert('Please select a firmware (.bin) file first.');
        return;
    }

    const backupInput = document.getElementById('backupFileInput');
    if (action === 'restore_config' && (!backupInput.files || !backupInput.files[0])) {
        alert('Please select a backup (.tar.gz) file first.');
        return;
    }

    const deviceType = document.getElementById('deviceType').value;
    const passwords = document.getElementById('passwords').value;
    const macs = document.getElementById('macInput').value;
    const username = document.getElementById('username').value;
    const port = document.getElementById('port').value;
    const workers = document.getElementById('workers').value;

    if (!isRetry) {
        pendingLogNodes = [];
        pendingResults = [];
        pendingProgress = null;
        if (rafId) {
            cancelAnimationFrame(rafId);
            rafId = null;
        }
        prepareTableRows(ips);
        document.getElementById('retryFailedBtn').style.display = 'none';
        document.getElementById('openExcelBtn').style.display = 'none';
        document.getElementById('exportRmsBtn').style.display = 'none';
        document.getElementById('exportStatusText').innerText = '';
        document.getElementById('logsContainer').innerHTML = '';
        storedLogs = [];
        executionStartTime = Date.now();
    } else {
        execFailed = Math.max(0, execFailed - ips.length);
        ips.forEach(item => {
            const candidateRows = Array.from(document.querySelectorAll('#resultsBody tr[data-ip]')).filter(r => r.getAttribute('data-ip') === item);
            const tr = candidateRows.find(r => r.getAttribute('data-status') === 'Failed') || candidateRows[0];
            if (tr) {
                tr.setAttribute('data-status', 'Pending');
                tr.children[2].innerHTML = '<span class="badge badge-pending">Retrying</span>';
            }
        });
        updateSummaryMetrics();
    }

    isCancelling = false;
    hideCancellingState();
    setEngineStatus('Executing...', 'running');
    document.getElementById('runBtn').style.display = 'none';
    const stopBtn = document.getElementById('stopBtn');
    stopBtn.style.display = 'inline-flex';
    stopBtn.disabled = false;
    stopBtn.innerText = 'Cancel Operation';
    document.getElementById('progressBox').style.display = 'block';
    document.getElementById('progressBar').style.width = '0%';
    document.getElementById('progressBar').style.background = 'var(--progress-active)';

    const formData = new FormData();
    formData.append('mode', action);
    formData.append('deviceType', deviceType);
    formData.append('ips', ips.join('\n'));
    formData.append('passwords', passwords);
    formData.append('mac_addresses', macs);
    formData.append('username', username);
    formData.append('port', port);
    formData.append('workers', workers);
    formData.append('is_retry', isRetry ? 'true' : 'false');

    if (action === 'update_firmware' && fileInput.files && fileInput.files[0]) {
        formData.append('firmware_file', fileInput.files[0]);
    }
    if (action === 'restore_config' && backupInput.files && backupInput.files[0]) {
        formData.append('backup_file', backupInput.files[0]);
    }

    try {
        const res = await fetch('/api/start', { method: 'POST', body: formData });
        if (!res.ok) {
            let errMsg = 'Failed to start automation';
            try {
                const err = await res.json();
                errMsg = err.error || err.message || errMsg;
            } catch (_) {
                errMsg = (await res.text()) || `HTTP ${res.status}`;
            }
            alert(errMsg);
            resetExecutionUI();
            setEngineStatus('Engine Ready', '');
            return;
        }
        startLogStream();
    } catch (e) {
        alert('Request failed: ' + (e.message || e));
        resetExecutionUI();
        setEngineStatus('Engine Ready', '');
    }
}

function showCancellingState() {
    isCancelling = true;
    const overlay = document.getElementById('cancellingOverlay');
    if (overlay) overlay.style.display = 'flex';
    const stopBtn = document.getElementById('stopBtn');
    if (stopBtn) {
        stopBtn.disabled = true;
        stopBtn.innerHTML = '<span class="spinner-sm"></span> Trying to cancel...';
    }
    setEngineStatus('Cancelling...', 'cancelling');
}

function hideCancellingState() {
    isCancelling = false;
    const overlay = document.getElementById('cancellingOverlay');
    if (overlay) overlay.style.display = 'none';
    const stopBtn = document.getElementById('stopBtn');
    if (stopBtn) {
        stopBtn.disabled = false;
        stopBtn.innerText = 'Cancel Operation';
    }
}

async function cancelSequence() {
    if (isCancelling) return;
    showCancellingState();
    appendConsoleLog('[INFO] Cancellation requested. Aborting workers and finalizing...');
    try {
        await fetch('/api/stop', { method: 'POST' });
    } catch (_) {}
    setTimeout(() => {
        if (isCancelling) {
            hideCancellingState();
            finishExecution();
        }
    }, 5000);
}

function scheduleDomFlush() {
    if (!rafId) {
        rafId = requestAnimationFrame(flushDomUpdates);
    }
}

function flushDomUpdates() {
    rafId = null;

    if (pendingResults.length > 0) {
        const batch = pendingResults;
        pendingResults = [];
        let lastIp = '';
        for (let i = 0; i < batch.length; i++) {
            lastIp = applyResultPayload(batch[i]);
        }
        updateSummaryMetrics();
        if (autoScrollEnabled && lastIp) {
            const candidateRows = Array.from(document.querySelectorAll('#resultsBody tr[data-ip]')).filter(r => r.getAttribute('data-ip') === lastIp);
            const tr = candidateRows.find(r => r.getAttribute('data-status') !== 'Pending') || candidateRows[0];
            if (tr) tr.scrollIntoView({ block: 'nearest', behavior: 'auto' });
        }
    }

    if (pendingProgress !== null) {
        document.getElementById('progressBar').style.width = `${pendingProgress}%`;
        pendingProgress = null;
        updateSummaryMetrics();
    }

    if (pendingLogNodes.length > 0) {
        const container = document.getElementById('logsContainer');
        const frag = document.createDocumentFragment();
        for (let i = 0; i < pendingLogNodes.length; i++) {
            frag.appendChild(pendingLogNodes[i]);
        }
        pendingLogNodes = [];
        container.appendChild(frag);

        while (container.children.length > 1500) {
            container.removeChild(container.firstElementChild);
        }

        if (autoScrollEnabled) {
            container.scrollTop = container.scrollHeight;
        }
    }
}

function startLogStream() {
    if (eventSource) eventSource.close();
    eventSource = new EventSource('/api/stream');

    eventSource.onmessage = (event) => {
        const data = event.data;
        if (!data) return;

        if (data.startsWith('[UPDATE_PROGRESS]')) {
            const pct = data.replace('[UPDATE_PROGRESS]', '').trim();
            const bar = document.getElementById('updateProgressBar');
            const lbl = document.getElementById('updateProgressPercent');
            if (bar) bar.style.width = pct;
            if (lbl) lbl.innerText = pct;
            return;
        } else if (data.startsWith('[UPDATE_STATUS]')) {
            const msg = data.replace('[UPDATE_STATUS]', '').trim();
            const lbl = document.getElementById('updateProgressLabel');
            if (lbl) lbl.innerText = msg;
            return;
        } else if (data.startsWith('[UPDATE_SUCCESS]')) {
            const msg = data.replace('[UPDATE_SUCCESS]', '').trim();
            const lbl = document.getElementById('updateProgressLabel');
            if (lbl) lbl.innerText = msg;
            const banner = document.getElementById('updateBannerText');
            if (banner) banner.innerText = msg;
            return;
        } else if (data.startsWith('[UPDATE_ERROR]')) {
            const err = data.replace('[UPDATE_ERROR]', '').trim();
            alert('Update error: ' + err);
            return;
        }

        if (data.startsWith('PROGRESS:')) {
            const parts = data.substring(9).split('/');
            const cur = parseInt(parts[0]);
            const tot = parseInt(parts[1]);
            pendingProgress = tot > 0 ? (cur / tot) * 100 : 0;
            scheduleDomFlush();
        } else if (data.startsWith('SCP_PROG:')) {
            const raw = data.substring(9);
            const pipeIdx = raw.indexOf('|');
            if (pipeIdx !== -1) {
                const ip = raw.substring(0, pipeIdx);
                const msg = raw.substring(pipeIdx + 1);
                updateProgressLog(ip, msg);
            } else {
                appendConsoleLog(data);
            }
        } else if (data.startsWith('LIVE:')) {
            appendConsoleLog(`[INFO] ${data.substring(5)}`);
        } else if (data.startsWith('CANCEL_REQUESTED')) {
            showCancellingState();
        } else if (data.startsWith('RESULT:')) {
            pendingResults.push(data.substring(7));
            scheduleDomFlush();
        } else if (data.startsWith('DONE:')) {
            flushDomUpdates();
            const filename = data.substring(5).trim();
            finishExecution(filename);
        } else {
            appendConsoleLog(data);
        }
    };

    eventSource.onerror = () => {
        eventSource.close();
        setEngineStatus('Engine Ready', '');
        resetExecutionUI();
    };
}

function applyResultPayload(raw) {
    const parts = raw.split('|');
    const ip = parts[0];
    const status = parts[1] || 'Failed';
    const isSuccess = status.startsWith('Success');
    const badgeClass = isSuccess ? 'badge-success' : 'badge-danger';
    const action = document.getElementById('action').value;
    const details = parts.slice(2);

    const candidateRows = Array.from(document.querySelectorAll('#resultsBody tr[data-ip]')).filter(r => r.getAttribute('data-ip') === ip);
    const tr = candidateRows.find(r => r.getAttribute('data-status') === 'Pending') || candidateRows[0];
    if (tr) {
        const prevStatus = tr.getAttribute('data-status');
        if (prevStatus === 'Success') {
            if (!isSuccess) {
                execSuccess--;
                execFailed++;
            }
        } else if (prevStatus === 'Failed') {
            if (isSuccess) {
                execFailed--;
                execSuccess++;
            }
        } else {
            if (isSuccess) execSuccess++;
            else execFailed++;
        }
        tr.setAttribute('data-status', isSuccess ? 'Success' : 'Failed');
        tr.children[2].innerHTML = `<span class="badge ${badgeClass}">${status}</span>`;

        if (details.length > 0) {
            while (tr.children.length > 3) {
                tr.removeChild(tr.lastChild);
            }

            if (action === 'check_wifi') {
                const tdMac = document.createElement('td');
                tdMac.className = 'copyable';
                tdMac.innerHTML = isSuccess ? (details[0] || '-').replace(/\n/g, '<br>') : '-';
                tr.appendChild(tdMac);

                const ssidPairs = isSuccess ? details.slice(1) : [];
                const tdSsid = document.createElement('td');
                tdSsid.className = 'copyable';
                let ssidLines = [];
                let idx = 1;
                for (let i = 0; i < ssidPairs.length; i += 2) {
                    const s = ssidPairs[i];
                    const p = ssidPairs[i + 1];
                    if (s && s !== '-' && s !== 'N/A') {
                        ssidLines.push(`SSID${idx}: ${s} (Pass: ${p || 'N/A'})`);
                        idx++;
                    }
                }
                if (isSuccess) {
                    tdSsid.innerHTML = ssidLines.length > 0 ? ssidLines.join('<br>') : '-';
                } else {
                    tdSsid.innerText = details[0] || 'Failed';
                }
                tr.appendChild(tdSsid);

                const tdAction = document.createElement('td');
                tdAction.style.display = 'flex';
                tdAction.style.gap = '6px';
                if (isSuccess && details[0] && details[0] !== '-' && details[0] !== 'None') {
                    const btnMac = document.createElement('button');
                    btnMac.className = 'btn btn-secondary btn-sm';
                    btnMac.innerText = 'Copy MAC';
                    btnMac.onclick = function() {
                        copyMac(this, details[0]);
                    };
                    tdAction.appendChild(btnMac);
                }
                if (isSuccess && ssidPairs.length > 0) {
                    const btn = document.createElement('button');
                    btn.className = 'btn btn-secondary btn-sm';
                    btn.innerText = 'Copy SSID';
                    btn.onclick = function() {
                        copySsids(this, ...ssidPairs);
                    };
                    tdAction.appendChild(btn);
                }
                if (tdAction.children.length === 0) {
                    tdAction.innerText = '-';
                }
                tr.appendChild(tdAction);
            } else if (action === 'add_mac') {
                const tdMac = document.createElement('td');
                tdMac.className = 'copyable';
                tdMac.innerHTML = isSuccess ? (details[0] || '-').replace(/\n/g, '<br>') : '-';
                tr.appendChild(tdMac);

                const tdDetail = document.createElement('td');
                tdDetail.className = 'copyable';
                tdDetail.innerText = isSuccess ? (details[1] || 'Added') : (details[0] || 'Failed');
                tr.appendChild(tdDetail);

                const tdAction = document.createElement('td');
                if (isSuccess && details[0] && details[0] !== '-' && details[0] !== 'None') {
                    const btnMac = document.createElement('button');
                    btnMac.className = 'btn btn-secondary btn-sm';
                    btnMac.innerText = 'Copy MAC';
                    btnMac.onclick = function() {
                        copyMac(this, details[0]);
                    };
                    tdAction.appendChild(btnMac);
                } else {
                    tdAction.innerText = '-';
                }
                tr.appendChild(tdAction);
            } else if (action === 'extract_data') {
                const tdFw = document.createElement('td');
                tdFw.className = 'copyable';
                tdFw.innerText = isSuccess ? (details[0] || '-') : (details[0] || 'Failed');
                tr.appendChild(tdFw);

                const tdSn = document.createElement('td');
                tdSn.className = 'copyable';
                tdSn.innerText = isSuccess ? (details[1] || '-') : '-';
                tr.appendChild(tdSn);

                const tdMac = document.createElement('td');
                tdMac.className = 'copyable';
                tdMac.innerText = isSuccess ? (details[2] || '-') : '-';
                tr.appendChild(tdMac);

                const tdImei = document.createElement('td');
                tdImei.className = 'copyable';
                tdImei.innerText = isSuccess ? (details[3] || '-') : '-';
                tr.appendChild(tdImei);

                const tdIccid = document.createElement('td');
                tdIccid.className = 'copyable';
                tdIccid.innerText = isSuccess ? (details[4] || '-') : '-';
                tr.appendChild(tdIccid);

                const tdImsi = document.createElement('td');
                tdImsi.className = 'copyable';
                tdImsi.innerText = isSuccess ? (details[5] || '-') : '-';
                tr.appendChild(tdImsi);

                const tdOperator = document.createElement('td');
                tdOperator.className = 'copyable';
                tdOperator.innerText = isSuccess ? (details[6] || '-') : '-';
                tr.appendChild(tdOperator);

                const tdRssi = document.createElement('td');
                tdRssi.className = 'copyable';
                tdRssi.innerText = isSuccess ? (details[7] || '-') : '-';
                tr.appendChild(tdRssi);

                const tdRsrp = document.createElement('td');
                tdRsrp.className = 'copyable';
                tdRsrp.innerText = isSuccess ? (details[8] || '-') : '-';
                tr.appendChild(tdRsrp);

                const tdRsrq = document.createElement('td');
                tdRsrq.className = 'copyable';
                tdRsrq.innerText = isSuccess ? (details[9] || '-') : '-';
                tr.appendChild(tdRsrq);

                const tdSinr = document.createElement('td');
                tdSinr.className = 'copyable';
                tdSinr.innerText = isSuccess ? (details[10] || '-') : '-';
                tr.appendChild(tdSinr);

                const tdAction = document.createElement('td');
                tdAction.style.whiteSpace = 'nowrap';
                const btnGroup = document.createElement('div');
                btnGroup.style.display = 'inline-flex';
                btnGroup.style.gap = '6px';
                btnGroup.style.alignItems = 'center';

                const btnRetry = document.createElement('button');
                btnRetry.type = 'button';
                btnRetry.className = 'btn btn-secondary btn-sm';
                btnRetry.style.padding = '3px 8px';
                btnRetry.style.fontSize = '11px';
                btnRetry.style.lineHeight = '1';
                btnRetry.title = 'Retry extraction for ' + ip;
                btnRetry.innerHTML = '<svg class="ui-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="margin-right: 3px;"><polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/></svg>Retry';
                btnRetry.onclick = function() {
                    retrySingleDevice(ip, this);
                };

                const btnReboot = document.createElement('button');
                btnReboot.type = 'button';
                btnReboot.className = 'btn btn-danger btn-sm';
                btnReboot.style.padding = '3px 8px';
                btnReboot.style.fontSize = '11px';
                btnReboot.style.lineHeight = '1';
                btnReboot.title = 'Reboot device ' + ip;
                btnReboot.innerHTML = '<svg class="ui-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="margin-right: 3px;"><path d="M18.36 6.64a9 9 0 1 1-12.73 0"/><line x1="12" y1="2" x2="12" y2="12"/></svg>Reboot';
                btnReboot.onclick = function() {
                    promptRebootDevice(ip, this);
                };

                btnGroup.appendChild(btnRetry);
                btnGroup.appendChild(btnReboot);
                tdAction.appendChild(btnGroup);
                tr.appendChild(tdAction);
            } else if (action === 'extract_data_ap') {
                const tdFw = document.createElement('td');
                tdFw.className = 'copyable';
                tdFw.innerText = isSuccess ? (details[0] || '-') : '-';
                tr.appendChild(tdFw);

                const tdSn = document.createElement('td');
                tdSn.className = 'copyable';
                tdSn.innerText = isSuccess ? (details[1] || '-') : '-';
                tr.appendChild(tdSn);

                const tdMac = document.createElement('td');
                tdMac.className = 'copyable';
                tdMac.innerText = isSuccess ? (details[2] || '-') : '-';
                tr.appendChild(tdMac);

                const ssidPairs = isSuccess ? details.slice(3) : [];
                const tdSsid = document.createElement('td');
                tdSsid.className = 'copyable';
                let ssidLines = [];
                let idx = 1;
                for (let i = 0; i < ssidPairs.length; i += 2) {
                    const s = ssidPairs[i];
                    const p = ssidPairs[i + 1];
                    if (s && s !== '-' && s !== 'N/A') {
                        ssidLines.push(`SSID${idx}: ${s} (Pass: ${p || 'N/A'})`);
                        idx++;
                    }
                }
                if (isSuccess) {
                    tdSsid.innerHTML = ssidLines.length > 0 ? ssidLines.join('<br>') : '-';
                } else {
                    tdSsid.innerText = details[0] || 'Failed';
                }
                tr.appendChild(tdSsid);

                const tdAction = document.createElement('td');
                if (isSuccess && ssidPairs.length > 0) {
                    const btn = document.createElement('button');
                    btn.className = 'btn btn-secondary btn-sm';
                    btn.innerText = 'Copy SSID';
                    btn.onclick = function() {
                        copySsids(this, ...ssidPairs);
                    };
                    tdAction.appendChild(btn);
                } else {
                    tdAction.innerText = '-';
                }
                tr.appendChild(tdAction);
            } else if (action === 'bundle_check') {
                const tdSn = document.createElement('td');
                tdSn.className = 'copyable';
                tdSn.innerText = isSuccess ? (details[0] || '-') : '-';
                tr.appendChild(tdSn);

                const tdMac = document.createElement('td');
                tdMac.className = 'copyable';
                tdMac.innerText = isSuccess ? (details[1] || '-') : '-';
                tr.appendChild(tdMac);

                const tdPwd = document.createElement('td');
                tdPwd.className = 'copyable';
                tdPwd.innerText = isSuccess ? (details[2] || '-') : (details[0] || 'Failed');
                tr.appendChild(tdPwd);
            } else if (action === 'standardize') {
                const tdOld = document.createElement('td');
                tdOld.className = 'copyable';
                tdOld.innerText = details[0] || '-';
                tr.appendChild(tdOld);

                const tdNew = document.createElement('td');
                tdNew.className = 'copyable';
                tdNew.innerText = isSuccess ? (details[1] || '-') : '-';
                tr.appendChild(tdNew);

                const tdDetail = document.createElement('td');
                tdDetail.className = 'copyable';
                tdDetail.innerText = isSuccess ? (details[2] || 'Standardized successfully') : (details[2] || details[0] || 'Failed');
                tr.appendChild(tdDetail);
            } else if (action === 'update_firmware') {
                const tdPwd = document.createElement('td');
                tdPwd.className = 'copyable';
                tdPwd.innerText = details[0] || '-';
                tr.appendChild(tdPwd);

                const tdDetail = document.createElement('td');
                tdDetail.className = 'copyable';
                tdDetail.innerText = isSuccess ? (details[1] || 'Updated') : (details[1] || details[0] || 'Failed');
                tr.appendChild(tdDetail);
            } else if (action === 'backup_config') {
                const tdFile = document.createElement('td');
                tdFile.className = 'copyable';
                tdFile.innerText = isSuccess ? (details[0] || '-') : '-';
                tr.appendChild(tdFile);

                const tdFolder = document.createElement('td');
                tdFolder.className = 'copyable';
                tdFolder.innerText = isSuccess ? (details[1] || '-') : '-';
                tr.appendChild(tdFolder);

                const tdSize = document.createElement('td');
                tdSize.className = 'copyable';
                tdSize.innerText = isSuccess ? (details[2] || '-') : (details[2] || details[0] || 'Failed');
                tr.appendChild(tdSize);
            } else if (action === 'verify') {
                const tdPwd = document.createElement('td');
                tdPwd.className = 'copyable';
                tdPwd.innerText = isSuccess ? (details[0] || '-') : '-';
                tr.appendChild(tdPwd);

                const tdDetail = document.createElement('td');
                tdDetail.className = 'copyable';
                tdDetail.innerText = isSuccess ? 'Verified successfully' : (details[0] || 'Failed');
                tr.appendChild(tdDetail);
            } else {
                const tdDetail = document.createElement('td');
                tdDetail.className = 'copyable';
                tdDetail.innerText = details[0] || (isSuccess ? 'Success' : 'Failed');
                tr.appendChild(tdDetail);
            }
        }
        if (activeFilter === 'success') {
            tr.style.display = isSuccess ? '' : 'none';
        } else if (activeFilter === 'failed') {
            tr.style.display = isSuccess ? 'none' : '';
        }
    }
    return ip;
}

function matchesCurrentLogFilter(level, text) {
    if (activeLogFilter !== 'all' && level !== activeLogFilter) return false;
    if (logSearchQuery && !text.includes(logSearchQuery)) return false;
    return true;
}

function setLogFilter(filter) {
    activeLogFilter = filter;
    document.querySelectorAll('.log-filter-btn').forEach(btn => {
        btn.classList.toggle('active', btn.getAttribute('data-filter') === filter);
    });
    applyLogFilter();
}

function applyLogFilter() {
    logSearchQuery = (document.getElementById('logSearchInput').value || '').toLowerCase().trim();
    const lines = document.querySelectorAll('#logsContainer .log-line');
    lines.forEach(line => {
        const lvl = line.getAttribute('data-level') || 'info';
        const txt = line.getAttribute('data-text') || '';
        line.style.display = matchesCurrentLogFilter(lvl, txt) ? '' : 'none';
    });
}

function toggleAutoScroll() {
    autoScrollEnabled = !autoScrollEnabled;
    const btn = document.getElementById('autoScrollToggleBtn');
    if (btn) {
        btn.innerText = autoScrollEnabled ? 'Auto-scroll: ON' : 'Auto-scroll: OFF';
    }
}

function exportLogsToFile() {
    if (storedLogs.length === 0) {
        alert('No logs to export.');
        return;
    }
    const text = storedLogs.map(l => `[${l.time}] ${l.msg}`).join('\n');
    const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
    a.download = `teltonika_logs_${timestamp}.txt`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

function clearLogs() {
    storedLogs = [];
    pendingLogNodes = [];
    document.getElementById('logsContainer').innerHTML = '';
}

function appendConsoleLog(msg) {
    const time = new Date().toLocaleTimeString();
    const rawLower = msg.toLowerCase();
    let level = 'info';
    if (rawLower.includes('[error]') || rawLower.includes('failed')) level = 'error';
    else if (rawLower.includes('[success]')) level = 'success';
    else if (rawLower.includes('[debug]')) level = 'debug';

    storedLogs.push({ time, msg, level });

    let formatted = msg
        .replace(/\[SUCCESS\]/g, `<span class="badge badge-success" style="font-size:9px;margin-right:4px;">SUCCESS</span>`)
        .replace(/\[ERROR\]/g, `<span class="badge badge-danger" style="font-size:9px;margin-right:4px;">ERROR</span>`)
        .replace(/\[INFO\]/g, `<span class="badge badge-pending" style="font-size:9px;margin-right:4px;">INFO</span>`)
        .replace(/\[DEBUG\]/g, `<span style="color:#94a3b8;font-size:9px;margin-right:4px;">[DEBUG]</span>`);

    const line = document.createElement('div');
    line.className = 'log-line';
    line.setAttribute('data-level', level);
    line.setAttribute('data-text', msg.toLowerCase());
    line.innerHTML = `<span style="color:var(--text-muted);margin-right:8px;font-size:10px;font-family:var(--font-mono);">${time}</span>${formatted}`;

    if (!matchesCurrentLogFilter(level, msg.toLowerCase())) {
        line.style.display = 'none';
    }

    pendingLogNodes.push(line);
    scheduleDomFlush();
}

function updateProgressLog(ip, msg) {
    const container = document.getElementById('logsContainer');
    const safeId = 'scp_prog_' + ip.replace(/[^a-zA-Z0-9_-]/g, '_');
    let line = document.getElementById(safeId);
    const time = new Date().toLocaleTimeString();
    let formatted = msg
        .replace(/\[SUCCESS\]/g, `<span class="badge badge-success" style="font-size:9px;margin-right:4px;">SUCCESS</span>`)
        .replace(/\[ERROR\]/g, `<span class="badge badge-danger" style="font-size:9px;margin-right:4px;">ERROR</span>`)
        .replace(/\[INFO\]/g, `<span class="badge badge-pending" style="font-size:9px;margin-right:4px;">INFO</span>`)
        .replace(/\[DEBUG\]/g, `<span style="color:#94a3b8;font-size:9px;margin-right:4px;">[DEBUG]</span>`);

    if (!line) {
        line = document.createElement('div');
        line.id = safeId;
        line.className = 'log-line';
        line.setAttribute('data-level', 'info');
        line.setAttribute('data-text', msg.toLowerCase());
        container.appendChild(line);
    }
    line.innerHTML = `<span style="color:var(--text-muted);margin-right:8px;font-size:10px;font-family:var(--font-mono);">${time}</span>${formatted}`;
    if (autoScrollEnabled) {
        container.scrollTop = container.scrollHeight;
    }
}

function exportRmsCsv() {
    const rows = document.querySelectorAll('#resultsBody tr[data-ip]');
    if (rows.length === 0) {
        alert('No device results available to export.');
        return;
    }
    const action = document.getElementById('action').value;
    const csvLines = ['Device Name,Serial Number,MAC Address,Password'];
    let count = 0;

    rows.forEach(r => {
        const ip = r.getAttribute('data-ip') || '';
        const status = r.getAttribute('data-status') || '';
        if (status !== 'Success') return;

        const cells = Array.from(r.children).map(td => td.innerText.trim());
        let sn = '';
        let mac = '';
        let pwd = '';

        if (action === 'extract_data') {
            sn = cells[4] || '';
            mac = cells[5] || '';
            pwd = document.getElementById('passwords').value.split(',')[0].trim() || 'qwerty123';
        } else if (action === 'extract_data_ap') {
            sn = cells[4] || '';
            mac = cells[5] || '';
            pwd = document.getElementById('passwords').value.split(',')[0].trim() || 'qwerty123';
        } else if (action === 'bundle_check') {
            sn = cells[3] || '';
            mac = cells[4] || '';
            pwd = cells[5] || '';
        } else if (action === 'check_wifi' || action === 'add_mac') {
            mac = cells[3] || '';
            pwd = document.getElementById('passwords').value.split(',')[0].trim() || 'qwerty123';
        } else {
            pwd = cells[3] || document.getElementById('passwords').value.split(',')[0].trim() || 'qwerty123';
        }

        const cleanMac = mac.replace(/[\n\r]+/g, ' ').split(' ')[0].trim();
        if (sn || cleanMac) {
            csvLines.push(`"${ip}","${sn}","${cleanMac}","${pwd}"`);
            count++;
        }
    });

    if (count === 0) {
        alert('No devices with Serial Number or MAC Address found in results to export for RMS.');
        return;
    }

    const csvContent = csvLines.join('\r\n');
    const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
    a.download = `teltonika_rms_import_${timestamp}.csv`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

function finishExecution(filename) {
    flushDomUpdates();
    if (eventSource) {
        eventSource.close();
        eventSource = null;
    }
    const wasCancelled = isCancelling;
    hideCancellingState();
    resetExecutionUI();

    if (wasCancelled) {
        document.querySelectorAll('#resultsBody tr[data-status="Pending"]').forEach(tr => {
            tr.setAttribute('data-status', 'Cancelled');
            tr.children[2].innerHTML = '<span class="badge badge-pending">Cancelled</span>';
        });
        setEngineStatus('Cancelled', '');
    } else {
        setEngineStatus('Engine Ready', '');
    }

    updateSummaryMetrics();

    const progressBar = document.getElementById('progressBar');
    progressBar.style.width = '100%';

    const failedCount = execFailed;
    const successCount = execSuccess;

    if (wasCancelled) {
        progressBar.style.background = '#f59e0b';
    } else if (failedCount === 0) {
        progressBar.style.background = '#10b981';
    } else if (successCount > 0) {
        progressBar.style.background = '#f59e0b';
    } else {
        progressBar.style.background = '#ef4444';
    }

    if (filename) {
        latestExcelFilename = filename;
        document.getElementById('openExcelBtn').style.display = 'inline-flex';
        document.getElementById('exportStatusText').innerText = `Exported: ${filename}`;
    }

    if (successCount > 0) {
        const rmsBtn = document.getElementById('exportRmsBtn');
        if (rmsBtn) rmsBtn.style.display = 'inline-flex';
    }

    if (failedCount > 0) {
        document.getElementById('retryFailedBtn').style.display = 'inline-flex';
    }
}

function resetExecutionUI() {
    hideCancellingState();
    document.getElementById('runBtn').style.display = 'inline-flex';
    document.getElementById('stopBtn').style.display = 'none';
}

function retryFailedIps() {
    const failedIps = [];
    document.querySelectorAll('#resultsBody tr[data-ip]').forEach(tr => {
        if (tr.getAttribute('data-status') === 'Failed') {
            failedIps.push(tr.getAttribute('data-ip'));
        }
    });
    if (failedIps.length > 0) {
        executeSequence(failedIps);
    }
}

async function openLatestExcel() {
    if (!latestExcelFilename) return;
    try {
        await fetch(`/api/open/${encodeURIComponent(latestExcelFilename)}`);
    } catch (_) {}
}

let targetRebootIp = null;
let targetRebootPort = null;

function promptRebootDevice(ip, clickedEl) {
    if (!ip) return;
    targetRebootIp = ip;

    const row = clickedEl ? clickedEl.closest('tr') : Array.from(document.querySelectorAll('#resultsBody tr[data-ip]')).find(r => r.getAttribute('data-ip') === ip);
    const rowPort = row ? row.getAttribute('data-port') : null;
    const explicitPort = ip.includes(':') ? ip.split(':')[1] : null;
    const activePort = explicitPort || rowPort || document.getElementById('port')?.value || getSavedRouterPort();
    targetRebootPort = activePort;

    const ipDisplay = document.getElementById('rebootTargetIpDisplay');
    if (ipDisplay) ipDisplay.textContent = `${ip} (port ${activePort})`;
    const modal = document.getElementById('rebootConfirmModal');
    if (modal) modal.style.display = 'flex';
}

function closeRebootConfirmModal() {
    targetRebootIp = null;
    targetRebootPort = null;
    const modal = document.getElementById('rebootConfirmModal');
    if (modal) modal.style.display = 'none';
}

async function executeRebootDevice() {
    if (!targetRebootIp) return;
    const ipToReboot = targetRebootIp;
    const portToReboot = targetRebootPort || document.getElementById('port')?.value || getSavedRouterPort();
    const btn = document.getElementById('confirmRebootActionBtn');
    if (btn) {
        btn.disabled = true;
        btn.textContent = 'Rebooting...';
    }

    try {
        const username = document.getElementById('username')?.value || 'admin';
        const rawPwds = document.getElementById('passwords')?.value || '';
        const passwords = rawPwds.split(',').map(p => p.trim()).filter(Boolean);

        const res = await fetch('/api/reboot', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                ip: ipToReboot,
                username: username,
                port: portToReboot,
                passwords: passwords
            })
        });

        const rawText = await res.text().catch(() => '');
        let errMsg = rawText || `HTTP ${res.status}`;
        try {
            const parsed = JSON.parse(rawText);
            errMsg = parsed.message || parsed.error || errMsg;
        } catch (_) {}

        if (!res.ok) {
            alert(errMsg);
        } else {
            alert(`Reboot command sent to ${ipToReboot}: ${errMsg}`);
        }
    } catch (e) {
        alert('Reboot request error: ' + (e.message || e));
    } finally {
        if (btn) {
            btn.disabled = false;
            btn.textContent = 'Reboot Device';
        }
        closeRebootConfirmModal();
    }
}

function retrySingleDevice(ip, clickedEl) {
    if (!ip) return;
    const runBtn = document.getElementById('runBtn');
    if (runBtn && runBtn.style.display === 'none') {
        alert('An operation is already running. Please wait or cancel it before retrying a device.');
        return;
    }
    if (clickedEl) {
        const row = clickedEl.closest('tr');
        if (row) {
            row.setAttribute('data-status', 'Failed');
        }
    }
    executeSequence([ip]);
}
