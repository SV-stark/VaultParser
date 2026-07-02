import * as pdfjsLib from './lib/pdf.min.mjs';

// Configure PDF.js local worker
pdfjsLib.GlobalWorkerOptions.workerSrc = './lib/pdf.worker.min.mjs';

// Bank presets relative guides & mappings
const PRESETS = {
  auto: {
    name: '⚡ Auto-Detect (AI Gaps)',
    guides: null,
    mappings: []
  },
  hdfc: {
    name: '🏦 HDFC Bank India',
    guides: [0.11, 0.42, 0.52, 0.62, 0.75, 0.88],
    mappings: ['date', 'description', 'reference', 'skip', 'debit', 'credit', 'balance']
  },
  sbi: {
    name: '🏦 State Bank of India (SBI)',
    guides: [0.10, 0.20, 0.52, 0.64, 0.76, 0.88],
    mappings: ['date', 'skip', 'description', 'reference', 'debit', 'credit', 'balance']
  },
  canara: {
    name: '🏦 Canara Bank',
    guides: [0.12, 0.48, 0.60, 0.74, 0.87],
    mappings: ['date', 'description', 'reference', 'debit', 'credit', 'balance']
  },
  union: {
    name: '🏦 Union Bank of India',
    guides: [0.11, 0.42, 0.52, 0.62, 0.75, 0.88],
    mappings: ['date', 'description', 'reference', 'skip', 'debit', 'credit', 'balance']
  },
  uco: {
    name: '🏦 UCO Bank',
    guides: [0.12, 0.24, 0.60, 0.74, 0.87],
    mappings: ['date', 'reference', 'description', 'debit', 'credit', 'balance']
  },
  indian: {
    name: '🏦 Indian Bank',
    guides: [0.08, 0.18, 0.45, 0.58, 0.71, 0.80],
    mappings: ['date', 'value_date', 'description', 'chq_no', 'debit', 'credit', 'balance']
  },
  hpscb: {
    name: '🏦 H P State Co-operative Bank',
    guides: [0.08, 0.16, 0.38, 0.41, 0.65, 0.74, 0.82],
    mappings: ['s_no', 'date', 'value_date', 'chq_no', 'description', 'debit', 'credit', 'balance']
  },
  icici: {
    name: '🏦 ICICI Bank',
    guides: [0.08, 0.44, 0.55, 0.62, 0.88],
    mappings: ['date', 'description', 'chq_no', 'debit', 'credit', 'balance']
  },
  custom: {
    name: '⚙️ Custom Layout',
    guides: null,
    mappings: null
  }
};

// Application State
let file = null;
let pdfDoc = null;
let numPages = 0;
let currentPage = 1;
let viewportWidth = 600;
let viewportHeight = 800;
let textItems = [];

let selectedPreset = 'auto';
let colGuides = [];
let colMappings = [];
let yTolerance = 6;
let mergeDescriptions = true;
let skipHeaderRows = 0;
let skipFooterRows = 0;
let filterDate = true;
let filterAmount = false;

// Visual Page Crop (in percentage points)
let topCropPct = 0;
let bottomCropPct = 100;
let yTopTrim = 0;
let yBottomTrim = 800;

// Overrides map
let manualEdits = {}; // { [page]: { [y_key]: { [col]: val } } }
let deletedRows = {}; // { [page]: { [y_key]: true } }
let draggingType = null; // 'col', 'top_trim', 'bottom_trim'
let draggingIdx = null;

// DOM Elements
const uploadStage = document.getElementById('upload-stage');
const fileLoadedStage = document.getElementById('file-loaded-stage');
const dropZone = document.getElementById('drop-zone');
const fileInput = document.getElementById('file-input');
const btnLoadSample = document.getElementById('btn-load-sample');
const fileInfoName = document.getElementById('file-info-name');
const fileInfoSize = document.getElementById('file-info-size');
const btnRemoveFile = document.getElementById('btn-remove-file');

const btnPagePrev = document.getElementById('btn-page-prev');
const btnPageNext = document.getElementById('btn-page-next');
const currentPageNum = document.getElementById('current-page-num');
const totalPageNum = document.getElementById('total-page-num');

const presetsCard = document.getElementById('presets-card');
const presetSelector = document.getElementById('preset-selector');

const settingsCard = document.getElementById('settings-card');
const yToleranceInput = document.getElementById('y-tolerance-input');
const yToleranceVal = document.getElementById('y-tolerance-val');
const mergeDescriptionsInput = document.getElementById('merge-descriptions-input');
const skipHeaderInput = document.getElementById('skip-header-input');
const skipFooterInput = document.getElementById('skip-footer-input');
const filterDateInput = document.getElementById('filter-date-input');
const filterAmountInput = document.getElementById('filter-amount-input');

const toggleMergeLbl = document.getElementById('toggle-merge-lbl');
const toggleFilterDateLbl = document.getElementById('toggle-filter-date-lbl');
const toggleFilterAmountLbl = document.getElementById('toggle-filter-amount-lbl');

const mappingsCard = document.getElementById('mappings-card');
const mappingsList = document.getElementById('mappings-list');

const loadingCard = document.getElementById('loading-card');
const loadingMessage = document.getElementById('loading-message');
const emptyCard = document.getElementById('empty-card');
const workspaceCard = document.getElementById('workspace-card');

const pdfCanvas = document.getElementById('pdf-canvas');
const canvasWrapper = document.getElementById('canvas-wrapper');
const interactiveOverlay = document.getElementById('interactive-overlay');

const statsBadge = document.getElementById('stats-badge');
const btnExportCsv = document.getElementById('btn-export-csv');
const btnExportXlsx = document.getElementById('btn-export-xlsx');
const previewTable = document.getElementById('preview-table');
const tableHeadersRow = document.getElementById('table-headers-row');
const tableBody = document.getElementById('table-body');
const tableEmptyMessage = document.getElementById('table-empty-message');

// Initial Setup
bindEvents();

function bindEvents() {
  // Drag & drop
  dropZone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropZone.classList.add('active');
  });
  dropZone.addEventListener('dragleave', () => dropZone.classList.remove('active'));
  dropZone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropZone.classList.remove('active');
    const droppedFile = e.dataTransfer.files[0];
    if (droppedFile && droppedFile.type === 'application/pdf') {
      loadPDFFile(droppedFile);
    }
  });
  dropZone.addEventListener('click', () => fileInput.click());
  fileInput.addEventListener('change', (e) => {
    const selectedFile = e.target.files[0];
    if (selectedFile) loadPDFFile(selectedFile);
  });

  // Load sample statement
  btnLoadSample.addEventListener('click', loadSamplePDF);

  // Remove file
  btnRemoveFile.addEventListener('click', resetApplicationState);

  // Page navigation
  btnPagePrev.addEventListener('click', () => changePage(-1));
  btnPageNext.addEventListener('click', () => changePage(1));

  // Preset Selector
  presetSelector.addEventListener('change', (e) => handlePresetSelection(e.target.value));

  // Tuning Inputs
  yToleranceInput.addEventListener('input', (e) => {
    yTolerance = parseInt(e.target.value);
    yToleranceVal.textContent = yTolerance + 'px';
  });
  yToleranceInput.addEventListener('change', triggerConversion);

  mergeDescriptionsInput.addEventListener('change', (e) => {
    mergeDescriptions = e.target.checked;
    triggerConversion();
  });
  // Toggle visual states
  toggleMergeLbl.addEventListener('click', () => {
    mergeDescriptionsInput.checked = !mergeDescriptionsInput.checked;
    mergeDescriptionsInput.dispatchEvent(new Event('change'));
  });

  skipHeaderInput.addEventListener('change', (e) => {
    skipHeaderRows = Math.max(0, parseInt(e.target.value) || 0);
    triggerConversion();
  });
  skipFooterInput.addEventListener('change', (e) => {
    skipFooterRows = Math.max(0, parseInt(e.target.value) || 0);
    triggerConversion();
  });

  const topTrimInput = document.getElementById('top-trim-input');
  const bottomTrimInput = document.getElementById('bottom-trim-input');
  topTrimInput.addEventListener('change', (e) => {
    topCropPct = Math.max(0, Math.min(bottomCropPct - 2, parseInt(e.target.value) || 0));
    e.target.value = topCropPct;
    yTopTrim = Math.round((topCropPct / 100) * viewportHeight);
    drawInteractiveOverlay();
    triggerConversion();
  });
  bottomTrimInput.addEventListener('change', (e) => {
    bottomCropPct = Math.max(topCropPct + 2, Math.min(100, parseInt(e.target.value) || 100));
    e.target.value = bottomCropPct;
    yBottomTrim = Math.round((bottomCropPct / 100) * viewportHeight);
    drawInteractiveOverlay();
    triggerConversion();
  });

  filterDateInput.addEventListener('change', (e) => {
    filterDate = e.target.checked;
    triggerConversion();
  });
  toggleFilterDateLbl.addEventListener('click', () => {
    filterDateInput.checked = !filterDateInput.checked;
    filterDateInput.dispatchEvent(new Event('change'));
  });

  filterAmountInput.addEventListener('change', (e) => {
    filterAmount = e.target.checked;
    triggerConversion();
  });
  toggleFilterAmountLbl.addEventListener('click', () => {
    filterAmountInput.checked = !filterAmountInput.checked;
    filterAmountInput.dispatchEvent(new Event('change'));
  });

  // Overlay interactive events for draggable guides
  interactiveOverlay.addEventListener('mousemove', handleOverlayMouseMove);
  interactiveOverlay.addEventListener('mouseup', handleOverlayMouseUp);
  interactiveOverlay.addEventListener('mouseleave', handleOverlayMouseUp);
  interactiveOverlay.addEventListener('dblclick', handleOverlayDoubleClick);

  // Export buttons
  btnExportCsv.addEventListener('click', () => handleBulkExport('csv'));
  btnExportXlsx.addEventListener('click', () => handleBulkExport('xlsx'));
}

// Reset application to raw upload state
function resetApplicationState() {
  file = null;
  pdfDoc = null;
  numPages = 0;
  currentPage = 1;
  textItems = [];
  colGuides = [];
  colMappings = [];
  manualEdits = {};
  deletedRows = {};
  selectedPreset = 'auto';
  presetSelector.value = 'auto';
  topCropPct = 0;
  bottomCropPct = 100;
  yTopTrim = 0;
  yBottomTrim = 800;
  document.getElementById('top-trim-input').value = 0;
  document.getElementById('bottom-trim-input').value = 100;

  // Toggle DOM card states
  uploadStage.classList.remove('hidden');
  fileLoadedStage.classList.add('hidden');
  presetsCard.classList.add('hidden');
  settingsCard.classList.add('hidden');
  mappingsCard.classList.add('hidden');
  workspaceCard.classList.add('hidden');
  emptyCard.classList.remove('hidden');
}

// Show/Hide loader utility
function showLoading(msg) {
  loadingCard.classList.remove('hidden');
  loadingMessage.textContent = msg;
}
function hideLoading() {
  loadingCard.classList.add('hidden');
}

// Fetch and render local HDFC sample statement
async function loadSamplePDF() {
  showLoading('Fetching sample HDFC statement...');
  try {
    const response = await fetch('./sample_hdfc.pdf');
    const blob = await response.blob();
    const sampleFile = new File([blob], 'RKS_USER_CHARGES_bank.pdf', { type: 'application/pdf' });
    selectedPreset = 'hdfc';
    presetSelector.value = 'hdfc';
    loadPDFFile(sampleFile);
  } catch (err) {
    console.error(err);
    alert('Failed to load sample statement: ' + err.message);
    hideLoading();
  }
}

// Load pdf structure
function loadPDFFile(pdfFile) {
  file = pdfFile;
  showLoading('Reading PDF file...');
  currentPage = 1;
  manualEdits = {};
  deletedRows = {};
  colGuides = [];
  
  // Set UI labels
  fileInfoName.textContent = file.name;
  fileInfoSize.textContent = (file.size / 1024 / 1024).toFixed(2) + ' MB';
  
  uploadStage.classList.add('hidden');
  fileLoadedStage.classList.remove('hidden');
  
  const reader = new FileReader();
  reader.onload = async (e) => {
    try {
      showLoading('Loading document structure...');
      const typedArray = new Uint8Array(e.target.result);
      const loadingTask = pdfjsLib.getDocument({ data: typedArray });
      pdfDoc = await loadingTask.promise;
      
      numPages = pdfDoc.numPages;
      totalPageNum.textContent = numPages;
      
      // Render first page
      await renderPDFPage(currentPage);
    } catch (err) {
      console.error(err);
      alert('Failed to parse PDF: ' + err.message);
      resetApplicationState();
    } finally {
      hideLoading();
    }
  };
  reader.readAsArrayBuffer(file);
}

// Render selected PDF page to canvas and extract words layout
async function renderPDFPage(pageNum) {
  showLoading(`Rendering Page ${pageNum}...`);
  try {
    const page = await pdfDoc.getPage(pageNum);
    const scale = 1.5;
    const viewport = page.getViewport({ scale });
    
    viewportWidth = viewport.width;
    viewportHeight = viewport.height;
    
    // Set Canvas Dimensions
    pdfCanvas.width = viewportWidth;
    pdfCanvas.height = viewportHeight;
    canvasWrapper.style.width = viewportWidth + 'px';
    canvasWrapper.style.height = viewportHeight + 'px';
    
    // Set absolute trim lines positions based on percentages
    yTopTrim = Math.round((topCropPct / 100) * viewportHeight);
    yBottomTrim = Math.round((bottomCropPct / 100) * viewportHeight);
    
    const context = pdfCanvas.getContext('2d');
    const renderContext = {
      canvasContext: context,
      viewport: viewport
    };
    await page.render(renderContext).promise;
    
    // Extract text items layout coords
    showLoading('Parsing text coordinates...');
    const textContent = await page.getTextContent();
    textItems = textContent.items.map((item, idx) => {
      const [vx, vy] = viewport.convertToViewportPoint(item.transform[4], item.transform[5]);
      const textHeight = item.height || Math.abs(item.transform[3]);
      return {
        id: `item-${idx}`,
        str: item.str,
        x: vx,
        y: vy - (textHeight * scale),
        w: item.width * scale,
        h: textHeight * scale
      };
    });
    
    // Auto-detect preset based on text keywords if first page load and using 'auto'
    if (pageNum === 1 && selectedPreset === 'auto') {
      const fullText = textItems.map(it => it.str).join(' ').toUpperCase();
      let matchedPreset = 'auto';
      if (fullText.includes('HDFC BANK') || fullText.includes('HDFCBANK')) matchedPreset = 'hdfc';
      else if (fullText.includes('STATE BANK OF INDIA') || fullText.includes('SBI ')) matchedPreset = 'sbi';
      else if (fullText.includes('CANARA')) matchedPreset = 'canara';
      else if (fullText.includes('UNION BANK')) matchedPreset = 'union';
      else if (fullText.includes('UCO BANK')) matchedPreset = 'uco';
      else if (fullText.includes('INDIAN BANK') || fullText.includes('ALLAHABAD')) matchedPreset = 'indian';
      else if (fullText.includes('H P STATE CO-OP') || fullText.includes('CO-OPERATIVE BANK') || fullText.includes('HPSCB')) matchedPreset = 'hpscb';
      else if (fullText.includes('ICICI BANK')) matchedPreset = 'icici';
      
      if (matchedPreset !== 'auto') {
        selectedPreset = matchedPreset;
        presetSelector.value = matchedPreset;
        const preset = PRESETS[matchedPreset];
        colGuides = preset.guides.map(p => Math.round(p * viewportWidth));
        colMappings = [...preset.mappings];
      }
    }
    
    // Initialize guides if first page load
    if (colGuides.length === 0) {
      if (selectedPreset === 'auto') {
        // Filter out header/footer words before auto-detecting column gaps to ensure high accuracy
        const filteredForAuto = textItems.filter(it => yTopTrim <= it.y && it.y <= yBottomTrim);
        const autoGuides = autoDetectColumns(filteredForAuto, viewportWidth);
        colGuides = autoGuides;
        adjustMappings(autoGuides.length + 1);
      } else if (selectedPreset !== 'custom') {
        const preset = PRESETS[selectedPreset];
        colGuides = preset.guides.map(p => Math.round(p * viewportWidth));
        colMappings = [...preset.mappings];
      }
    }
    
    // Update side views
    presetsCard.classList.remove('hidden');
    settingsCard.classList.remove('hidden');
    emptyCard.classList.add('hidden');
    workspaceCard.classList.remove('hidden');
    
    // Re-draw guidelines & highlights
    drawInteractiveOverlay();
    
    // Fetch transaction table parsing results
    await triggerConversion();
    
    // Toggle page buttons disabled states
    btnPagePrev.disabled = pageNum <= 1;
    btnPageNext.disabled = pageNum >= numPages;
    currentPageNum.textContent = pageNum;
    
  } catch (err) {
    console.error(err);
    alert('Error rendering page: ' + err.message);
  } finally {
    hideLoading();
  }
}

// Column auto gap algorithm
function autoDetectColumns(items, width) {
  if (items.length === 0) return [];
  const resolution = Math.round(width);
  const coverage = new Array(resolution).fill(0);
  
  items.forEach(item => {
    const xStart = Math.max(0, Math.floor(item.x));
    const xEnd = Math.min(resolution - 1, Math.ceil(item.x + item.w));
    for (let i = xStart; i <= xEnd; i++) {
      coverage[i]++;
    }
  });
  
  const gaps = [];
  let inGap = false;
  let gapStart = 0;
  for (let i = 0; i < resolution; i++) {
    if (coverage[i] === 0 && !inGap) {
      inGap = true;
      gapStart = i;
    } else if (coverage[i] > 0 && inGap) {
      inGap = false;
      gaps.push({ start: gapStart, end: i - 1 });
    }
  }
  if (inGap) gaps.push({ start: gapStart, end: resolution - 1 });
  
  const minGapWidth = 12;
  const cleanGaps = gaps.filter(g => (g.end - g.start) >= minGapWidth);
  const boundaries = [];
  cleanGaps.forEach(g => {
    const center = (g.start + g.end) / 2;
    if (center > width * 0.05 && center < width * 0.95) {
      boundaries.push(Math.round(center));
    }
  });
  
  return boundaries.sort((a, b) => a - b);
}

// Sync mappings array count
function adjustMappings(newCount) {
  const next = [...colMappings];
  if (next.length < newCount) {
    while (next.length < newCount) next.push('skip');
  } else if (next.length > newCount) {
    next.splice(newCount);
  }
  colMappings = next;
}

// Page Navigation
async function changePage(dir) {
  const target = currentPage + dir;
  if (target >= 1 && target <= numPages) {
    currentPage = target;
    await renderPDFPage(currentPage);
  }
}

// Preset dropdown trigger
function handlePresetSelection(presetKey) {
  selectedPreset = presetKey;
  if (presetKey === 'auto') {
    const autoGuides = autoDetectColumns(textItems, viewportWidth);
    colGuides = autoGuides;
    adjustMappings(autoGuides.length + 1);
  } else if (presetKey !== 'custom') {
    const preset = PRESETS[presetKey];
    colGuides = preset.guides.map(p => Math.round(p * viewportWidth));
    colMappings = [...preset.mappings];
  }
  
  drawInteractiveOverlay();
  triggerConversion();
}

// Draw overlays elements on canvas wrapper (lines, rects)
function drawInteractiveOverlay() {
  interactiveOverlay.innerHTML = '';
  
  // 1. Draw text bounding highlights
  textItems.forEach(item => {
    const el = document.createElement('div');
    el.className = 'text-highlight';
    el.style.left = item.x + 'px';
    el.style.top = item.y + 'px';
    el.style.width = item.w + 'px';
    el.style.height = item.h + 'px';
    el.title = item.str;
    
    // Fade out text highlights that are cropped out
    if (item.y < yTopTrim || item.y > yBottomTrim) {
      el.style.opacity = '0.15';
    }
    
    interactiveOverlay.appendChild(el);
  });
  
  // 1b. Draw Top Trim Guide (horizontal)
  const topTrimEl = document.createElement('div');
  topTrimEl.className = 'trim-guide';
  topTrimEl.id = 'top-trim-guide';
  topTrimEl.style.top = yTopTrim + 'px';
  
  const topTrimLine = document.createElement('div');
  topTrimLine.className = 'trim-guide-line';
  
  const topTrimLabel = document.createElement('div');
  topTrimLabel.className = 'trim-guide-label';
  topTrimLabel.textContent = `Top Crop Limit (${topCropPct}%)`;
  
  topTrimEl.appendChild(topTrimLine);
  topTrimEl.appendChild(topTrimLabel);
  topTrimEl.addEventListener('mousedown', (e) => {
    e.stopPropagation();
    draggingType = 'top_trim';
    topTrimEl.classList.add('dragging');
  });
  interactiveOverlay.appendChild(topTrimEl);
  
  // 1c. Draw Bottom Trim Guide (horizontal)
  const bottomTrimEl = document.createElement('div');
  bottomTrimEl.className = 'trim-guide';
  bottomTrimEl.id = 'bottom-trim-guide';
  bottomTrimEl.style.top = yBottomTrim + 'px';
  
  const bottomTrimLine = document.createElement('div');
  bottomTrimLine.className = 'trim-guide-line';
  
  const bottomTrimLabel = document.createElement('div');
  bottomTrimLabel.className = 'trim-guide-label';
  bottomTrimLabel.textContent = `Bottom Crop Limit (${bottomCropPct}%)`;
  
  bottomTrimEl.appendChild(bottomTrimLine);
  bottomTrimEl.appendChild(bottomTrimLabel);
  bottomTrimEl.addEventListener('mousedown', (e) => {
    e.stopPropagation();
    draggingType = 'bottom_trim';
    bottomTrimEl.classList.add('dragging');
  });
  interactiveOverlay.appendChild(bottomTrimEl);
  
  // 2. Draw draggable vertical column dividers
  colGuides.forEach((guideX, idx) => {
    const el = document.createElement('div');
    el.className = 'col-guide';
    el.style.left = guideX + 'px';
    
    const line = document.createElement('div');
    line.className = 'col-guide-line';
    
    const handle = document.createElement('div');
    handle.className = 'col-guide-handle';
    handle.textContent = idx + 1;
    
    el.appendChild(line);
    el.appendChild(handle);
    
    // Drag handlers
    el.addEventListener('mousedown', (e) => {
      e.stopPropagation();
      draggingType = 'col';
      draggingIdx = idx;
      el.classList.add('dragging');
    });
    
    interactiveOverlay.appendChild(el);
  });
  
  // 3. Render mappings layout selectors
  renderMappingsControls();
}

// Renders the mapping cards select rows dynamically
function renderMappingsControls() {
  mappingsCard.classList.remove('hidden');
  mappingsList.innerHTML = '';
  
  const numCols = colGuides.length + 1;
  adjustMappings(numCols);
  
  for (let idx = 0; idx < numCols; idx++) {
    const row = document.createElement('div');
    row.className = 'column-mapping-row';
    
    const badge = document.createElement('div');
    badge.className = 'col-num-badge';
    badge.textContent = idx + 1;
    
    const select = document.createElement('select');
    const options = [
      { val: 'skip', label: 'Skip / Ignore' },
      { val: 'date', label: 'Transaction Date' },
      { val: 'description', label: 'Description / Narration' },
      { val: 'reference', label: 'Reference / Chq No.' },
      { val: 'amount', label: 'Amount (Combined)' },
      { val: 'debit', label: 'Withdrawals / Debit' },
      { val: 'credit', label: 'Deposits / Credit' },
      { val: 'balance', label: 'Account Balance' }
    ];
    
    options.forEach(opt => {
      const o = document.createElement('option');
      o.value = opt.val;
      o.textContent = opt.label;
      select.appendChild(o);
    });
    
    select.value = colMappings[idx] || 'skip';
    select.addEventListener('change', (e) => {
      selectedPreset = 'custom';
      presetSelector.value = 'custom';
      colMappings[idx] = e.target.value;
      triggerConversion();
    });
    
    row.appendChild(badge);
    row.appendChild(select);
    mappingsList.appendChild(row);
  }
}

// Overlay Drag Handlers
function handleOverlayMouseMove(e) {
  if (!draggingType) return;
  selectedPreset = 'custom';
  presetSelector.value = 'custom';
  
  const rect = interactiveOverlay.getBoundingClientRect();
  
  if (draggingType === 'col' && draggingIdx !== null) {
    let newX = e.clientX - rect.left;
    newX = Math.max(10, Math.min(viewportWidth - 10, newX));
    colGuides[draggingIdx] = Math.round(newX);
    const guidesDivs = interactiveOverlay.querySelectorAll('.col-guide');
    if (guidesDivs[draggingIdx]) {
      guidesDivs[draggingIdx].style.left = newX + 'px';
    }
  } else if (draggingType === 'top_trim') {
    let newY = e.clientY - rect.top;
    newY = Math.max(0, Math.min(yBottomTrim - 20, newY));
    yTopTrim = Math.round(newY);
    topCropPct = Math.round((yTopTrim / viewportHeight) * 100);
    const topTrimEl = document.getElementById('top-trim-guide');
    if (topTrimEl) {
      topTrimEl.style.top = newY + 'px';
      const label = topTrimEl.querySelector('.trim-guide-label');
      if (label) label.textContent = `Top Crop Limit (${topCropPct}%)`;
    }
    const topInput = document.getElementById('top-trim-input');
    if (topInput) topInput.value = topCropPct;
  } else if (draggingType === 'bottom_trim') {
    let newY = e.clientY - rect.top;
    newY = Math.max(yTopTrim + 20, Math.min(viewportHeight, newY));
    yBottomTrim = Math.round(newY);
    bottomCropPct = Math.round((yBottomTrim / viewportHeight) * 100);
    const bottomTrimEl = document.getElementById('bottom-trim-guide');
    if (bottomTrimEl) {
      bottomTrimEl.style.top = newY + 'px';
      const label = bottomTrimEl.querySelector('.trim-guide-label');
      if (label) label.textContent = `Bottom Crop Limit (${bottomCropPct}%)`;
    }
    const bottomInput = document.getElementById('bottom-trim-input');
    if (bottomInput) bottomInput.value = bottomCropPct;
  }
}

function handleOverlayMouseUp() {
  if (draggingType) {
    if (draggingType === 'col' && draggingIdx !== null) {
      const el = interactiveOverlay.querySelectorAll('.col-guide')[draggingIdx];
      if (el) el.classList.remove('dragging');
      draggingIdx = null;
      colGuides.sort((a, b) => a - b);
    } else if (draggingType === 'top_trim') {
      const el = document.getElementById('top-trim-guide');
      if (el) el.classList.remove('dragging');
      topCropPct = Math.round((yTopTrim / viewportHeight) * 100);
    } else if (draggingType === 'bottom_trim') {
      const el = document.getElementById('bottom-trim-guide');
      if (el) el.classList.remove('dragging');
      bottomCropPct = Math.round((yBottomTrim / viewportHeight) * 100);
    }
    
    draggingType = null;
    drawInteractiveOverlay();
    triggerConversion();
  }
}

function handleOverlayDoubleClick(e) {
  // Prevent double click on highlight boxes triggering adding lines
  if (e.target.className === 'text-highlight' || e.target.className === 'col-guide-handle') return;
  
  const rect = interactiveOverlay.getBoundingClientRect();
  const clickX = Math.round(e.clientX - rect.left);
  
  // Check proximity to existing guides
  const tooClose = colGuides.some(g => Math.abs(g - clickX) < 15);
  if (!tooClose) {
    selectedPreset = 'custom';
    presetSelector.value = 'custom';
    colGuides.push(clickX);
    colGuides.sort((a, b) => a - b);
    drawInteractiveOverlay();
    triggerConversion();
  }
}

// Call Rust converter endpoint and populate HTML table
async function triggerConversion() {
  if (!file) return;
  
  // Prepare guides as relative percentages
  const relGuides = colGuides.map(g => g / viewportWidth);
  
  const formData = new FormData();
  formData.append('file', file);
  formData.append('col_guides', JSON.stringify(relGuides));
  formData.append('col_mappings', JSON.stringify(colMappings));
  formData.append('y_tolerance', yTolerance);
  formData.append('merge_multi_line', mergeDescriptions);
  formData.append('skip_header_rows', skipHeaderRows);
  formData.append('skip_footer_rows', skipFooterRows);
  formData.append('filter_only_date', filterDate);
  formData.append('filter_only_amount', filterAmount);
  formData.append('manual_edits', JSON.stringify(manualEdits));
  formData.append('deleted_rows', JSON.stringify(deletedRows));
  formData.append('y_top_trim', (topCropPct / 100).toFixed(4));
  formData.append('y_bottom_trim', (bottomCropPct / 100).toFixed(4));
  formData.append('format', 'json');
  
  try {
    const res = await fetch('/api/convert', {
      method: 'POST',
      body: formData
    });
    
    if (!res.ok) throw new Error(await res.text());
    
    const data = await res.json();
    renderPreviewTable(data);
  } catch (err) {
    console.error(err);
    alert('Failed to parse statement values: ' + err.message);
  }
}

// Render table inside DOM preview
function renderPreviewTable(data) {
  tableHeadersRow.innerHTML = '';
  tableBody.innerHTML = '';
  
  // 1. Render column headers matching guides count
  const pgHeader = document.createElement('th');
  pgHeader.style.cssText = 'width:36px; color: var(--text-muted); font-size:0.65rem;';
  pgHeader.textContent = 'Pg';
  tableHeadersRow.appendChild(pgHeader);

  const trashHeader = document.createElement('th');
  trashHeader.style.width = '40px';
  tableHeadersRow.appendChild(trashHeader);
  
  const numCols = colGuides.length + 1;
  for (let idx = 0; idx < numCols; idx++) {
    const th = document.createElement('th');
    const type = colMappings[idx] || 'skip';
    th.innerHTML = `Col ${idx + 1}<span style="display: block; font-size: 0.65rem; color: ${type === 'skip' ? 'var(--text-muted)' : 'var(--secondary)'}; font-weight: normal; text-transform: capitalize; margin-top: 2px;">(${type})</span>`;
    tableHeadersRow.appendChild(th);
  }
  
  // Show all rows across all pages (page nav only moves the canvas view)
  const pageRows = data.rows || [];
  const currentPageRows = pageRows.filter(r => r.page === currentPage);
  statsBadge.textContent = `${pageRows.length} Rows (${currentPageRows.length} on page ${currentPage})`;

  if (pageRows.length === 0) {
    tableEmptyMessage.classList.remove('hidden');
    previewTable.classList.add('hidden');
    return;
  }
  
  tableEmptyMessage.classList.add('hidden');
  previewTable.classList.remove('hidden');
  
  pageRows.forEach(row => {
    const tr = document.createElement('tr');
    if (row.page === currentPage) tr.classList.add('current-page-row');
    
    // Page number badge cell
    const tdPage = document.createElement('td');
    tdPage.style.cssText = 'text-align:center; padding: 0.4rem 0.2rem;';
    const pgBadge = document.createElement('span');
    pgBadge.className = row.page === currentPage ? 'page-badge page-badge-active' : 'page-badge';
    pgBadge.textContent = row.page;
    tdPage.appendChild(pgBadge);
    tr.appendChild(tdPage);
    
    // Trash delete button cell
    const tdTrash = document.createElement('td');
    const btnTrash = document.createElement('button');
    btnTrash.className = 'btn-delete-row';
    btnTrash.textContent = '✕';
    btnTrash.title = 'Remove row';
    btnTrash.addEventListener('click', () => deleteRow(row.y));
    tdTrash.appendChild(btnTrash);
    tr.appendChild(tdTrash);
    
    row.cells.forEach((cellText, colIdx) => {
      const td = document.createElement('td');
      td.className = 'cell-editable';
      td.textContent = cellText || '';
      
      // Inline edit on double-click
      td.addEventListener('dblclick', () => {
        const inp = document.createElement('input');
        inp.type = 'text';
        inp.value = td.textContent;
        inp.style.width = '100%';
        td.textContent = '';
        td.appendChild(inp);
        inp.focus();
        
        const saveEdit = () => {
          const newVal = inp.value.trim();
          td.innerHTML = '';
          td.textContent = newVal;
          saveLocalCellEdit(row.y, colIdx, newVal);
        };
        
        inp.addEventListener('blur', saveEdit);
        inp.addEventListener('keydown', (e) => {
          if (e.key === 'Enter') saveEdit();
          if (e.key === 'Escape') {
            td.innerHTML = '';
            td.textContent = cellText;
          }
        });
      });
      
      tr.appendChild(td);
    });
    
    tableBody.appendChild(tr);
  });
}

// Save cell edits locally and trigger conversions updates
function saveLocalCellEdit(rowY, colIdx, newVal) {
  const pageStr = String(currentPage);
  const yKey = rowY.toFixed(2);
  
  if (!manualEdits[pageStr]) manualEdits[pageStr] = {};
  if (!manualEdits[pageStr][yKey]) manualEdits[pageStr][yKey] = {};
  
  manualEdits[pageStr][yKey][colIdx] = newVal;
  triggerConversion();
}

// Remove row locally
function deleteRow(rowY) {
  const pageStr = String(currentPage);
  const yKey = rowY.toFixed(2);
  
  if (!deletedRows[pageStr]) deletedRows[pageStr] = {};
  deletedRows[pageStr][yKey] = true;
  triggerConversion();
}

// Handle bulk exports by calling Rust endpoints
async function handleBulkExport(format) {
  if (!file) return;
  showLoading('Generating export file...');
  
  const relGuides = colGuides.map(g => g / viewportWidth);
  const formData = new FormData();
  formData.append('file', file);
  formData.append('col_guides', JSON.stringify(relGuides));
  formData.append('col_mappings', JSON.stringify(colMappings));
  formData.append('y_tolerance', yTolerance);
  formData.append('merge_multi_line', mergeDescriptions);
  formData.append('skip_header_rows', skipHeaderRows);
  formData.append('skip_footer_rows', skipFooterRows);
  formData.append('filter_only_date', filterDate);
  formData.append('filter_only_amount', filterAmount);
  formData.append('manual_edits', JSON.stringify(manualEdits));
  formData.append('deleted_rows', JSON.stringify(deletedRows));
  formData.append('y_top_trim', (topCropPct / 100).toFixed(4));
  formData.append('y_bottom_trim', (bottomCropPct / 100).toFixed(4));
  formData.append('format', format);
  
  try {
    const res = await fetch('/api/convert', {
      method: 'POST',
      body: formData
    });
    
    if (!res.ok) throw new Error(await res.text());
    
    // Receive blob stream
    const blob = await res.blob();
    const cleanFileName = file.name.replace(/\.[^/.]+$/, "");
    const ext = format === 'xlsx' ? 'xlsx' : 'csv';
    const mime = format === 'xlsx' 
      ? 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' 
      : 'text/csv';
      
    const downloadBlob = new Blob([blob], { type: mime });
    const link = document.createElement('a');
    link.href = URL.createObjectURL(downloadBlob);
    link.setAttribute('download', `${cleanFileName}_converted.${ext}`);
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    
  } catch (err) {
    console.error(err);
    alert('Export failed: ' + err.message);
  } finally {
    hideLoading();
  }
}
