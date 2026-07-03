const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;

const state = {
  settings: null,
  queue: [],
  nextId: 1,
  processing: false,
};

const els = {
  dropzone: document.querySelector("#dropzone"),
  pickFiles: document.querySelector("#pick-files"),
  queue: document.querySelector("#queue"),
  processAll: document.querySelector("#process-all"),
  retryFailed: document.querySelector("#retry-failed"),
  clearDone: document.querySelector("#clear-done"),
  outputDir: document.querySelector("#output-dir"),
  pickOutput: document.querySelector("#pick-output"),
  openOutput: document.querySelector("#open-output"),
  emitPdf: document.querySelector("#emit-pdf"),
  emitJpeg: document.querySelector("#emit-jpeg"),
  stripObi: document.querySelector("#strip-obi"),
  cutRatio: document.querySelector("#cut-ratio"),
  cutLabel: document.querySelector("#cut-label"),
  templateMode: document.querySelector("#template-mode"),
  templatePath: document.querySelector("#template-path"),
  pickTemplate: document.querySelector("#pick-template"),
  resetTemplate: document.querySelector("#reset-template"),
};

function fileName(path) {
  return path.split(/[\\/]/).pop() || path;
}

function addPaths(paths) {
  const existing = new Set(state.queue.map((item) => item.path));
  for (const path of paths) {
    if (!path.toLowerCase().endsWith(".pdf") || existing.has(path)) continue;
    state.queue.push({
      id: state.nextId++,
      path,
      name: fileName(path),
      status: "pending",
      error: null,
      result: null,
    });
    existing.add(path);
  }
  renderQueue();
}

function labelForStatus(status) {
  return {
    pending: "待機中",
    processing: "処理中",
    done: "完了",
    failed: "失敗",
  }[status] || status;
}

function renderQueue() {
  els.queue.classList.toggle("empty", state.queue.length === 0);
  if (state.queue.length === 0) {
    els.queue.innerHTML = '<div class="empty-state">PDFがまだ追加されていません</div>';
    updateButtons();
    return;
  }

  els.queue.innerHTML = state.queue
    .map((item) => {
      const outputLinks = renderOutputLinks(item);
      return `
        <div class="queue-item" data-id="${item.id}">
          <div>
            <div class="file-name" title="${item.name}">${item.name}</div>
            <div class="file-path" title="${item.path}">${item.path}</div>
            ${outputLinks}
            ${item.error ? `<div class="error-text" title="${item.error}">${item.error}</div>` : ""}
          </div>
          <div class="status ${item.status}">${labelForStatus(item.status)}</div>
          <div class="item-actions">
            <button class="secondary retry-one" ${item.status === "processing" ? "disabled" : ""}>再実行</button>
            <button class="secondary remove-one" ${item.status === "processing" ? "disabled" : ""}>削除</button>
          </div>
        </div>
      `;
    })
    .join("");

  for (const row of els.queue.querySelectorAll(".queue-item")) {
    const id = Number(row.dataset.id);
    const item = state.queue.find((entry) => entry.id === id);
    for (const button of row.querySelectorAll(".open-result")) {
      button.addEventListener("click", async () => {
        const path = button.dataset.path;
        if (path) await invoke("open_path", { path });
      });
    }
    row.querySelector(".retry-one").addEventListener("click", () => processItems([id]));
    row.querySelector(".remove-one").addEventListener("click", () => {
      state.queue = state.queue.filter((item) => item.id !== id);
      renderQueue();
    });
    if (item?.result?.output_dir) {
      const dirButton = row.querySelector(".open-result-dir");
      dirButton?.addEventListener("click", async () => {
        await invoke("open_path", { path: item.result.output_dir });
      });
    }
  }

  updateButtons();
}

function renderOutputLinks(item) {
  if (item.status !== "done" || !item.result) return "";

  const links = [];
  if (item.result.pdf_path) {
    links.push(`
      <button class="result-link open-result" data-path="${escapeAttr(item.result.pdf_path)}" title="${escapeAttr(item.result.pdf_path)}">
        PDFを開く
      </button>
    `);
  }
  if (item.result.jpeg_path) {
    links.push(`
      <button class="result-link open-result" data-path="${escapeAttr(item.result.jpeg_path)}" title="${escapeAttr(item.result.jpeg_path)}">
        JPEGを開く
      </button>
    `);
  }
  if (item.result.output_dir) {
    links.push(`
      <button class="result-link open-result-dir" title="${escapeAttr(item.result.output_dir)}">
        フォルダ
      </button>
    `);
  }

  return `
    <div class="result-links">
      ${links.join("")}
    </div>
  `;
}

function escapeAttr(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function renderSettings() {
  const settings = state.settings;
  els.outputDir.value = settings.output_dir || "";
  els.emitPdf.checked = settings.emit_pdf;
  els.emitJpeg.checked = settings.emit_jpeg;
  els.stripObi.checked = settings.strip_obi;
  els.cutRatio.value = Math.round(settings.obi_cut_ratio * 100);
  els.cutLabel.textContent = `${els.cutRatio.value}%`;
  els.templateMode.textContent =
    settings.template_mode === "custom" ? "カスタムテンプレート" : "プリセット";
  els.templatePath.textContent = settings.custom_template_path || "";
  updateButtons();
}

function updateButtons() {
  const hasQueue = state.queue.length > 0;
  const hasFailed = state.queue.some((item) => item.status === "failed");
  const hasDone = state.queue.some((item) => item.status === "done");
  const canProcess = hasQueue && !state.processing && state.settings?.output_dir;
  els.processAll.disabled = !canProcess;
  els.retryFailed.disabled = !hasFailed || state.processing || !state.settings?.output_dir;
  els.clearDone.disabled = !hasDone || state.processing;
  els.openOutput.disabled = !state.settings?.output_dir;
}

async function persistSettings() {
  state.settings = await invoke("save_settings", { settings: state.settings });
  renderSettings();
}

async function processItems(ids) {
  if (state.processing) return;
  if (!state.settings.output_dir) {
    await chooseOutputDir();
    if (!state.settings.output_dir) return;
  }
  if (!state.settings.emit_pdf && !state.settings.emit_jpeg) {
    state.settings.emit_pdf = true;
    await persistSettings();
  }

  state.processing = true;
  updateButtons();

  for (const id of ids) {
    const item = state.queue.find((entry) => entry.id === id);
    if (!item) continue;
    item.status = "processing";
    item.error = null;
    renderQueue();

    try {
      item.result = await invoke("process_one", {
        request: {
          input_path: item.path,
          settings: state.settings,
        },
      });
      item.status = "done";
    } catch (error) {
      item.status = "failed";
      item.error = String(error);
    }
    renderQueue();
  }

  state.processing = false;
  updateButtons();
}

async function chooseOutputDir() {
  const selected = await invoke("choose_output_dir");
  if (selected) {
    state.settings.output_dir = selected;
    await persistSettings();
  }
}

async function initDragDrop() {
  if (!listen) return;
  await listen("tauri://drag-enter", () => els.dropzone.classList.add("dragging"));
  await listen("tauri://drag-leave", () => els.dropzone.classList.remove("dragging"));
  await listen("tauri://drag-drop", (event) => {
    els.dropzone.classList.remove("dragging");
    const paths = event.payload?.paths || [];
    addPaths(paths);
  });
}

async function init() {
  if (!invoke) {
    document.body.innerHTML = "<p>Tauri環境で起動してください。</p>";
    return;
  }

  state.settings = await invoke("load_settings");
  renderSettings();
  renderQueue();
  await initDragDrop();

  els.pickFiles.addEventListener("click", async () => addPaths(await invoke("choose_input_pdfs")));
  els.pickOutput.addEventListener("click", chooseOutputDir);
  els.openOutput.addEventListener("click", async () => {
    if (state.settings.output_dir) await invoke("open_path", { path: state.settings.output_dir });
  });
  els.processAll.addEventListener("click", () => {
    const ids = state.queue.filter((item) => item.status !== "processing").map((item) => item.id);
    processItems(ids);
  });
  els.retryFailed.addEventListener("click", () => {
    const ids = state.queue.filter((item) => item.status === "failed").map((item) => item.id);
    processItems(ids);
  });
  els.clearDone.addEventListener("click", () => {
    state.queue = state.queue.filter((item) => item.status !== "done");
    renderQueue();
  });

  els.emitPdf.addEventListener("change", async () => {
    state.settings.emit_pdf = els.emitPdf.checked;
    await persistSettings();
  });
  els.emitJpeg.addEventListener("change", async () => {
    state.settings.emit_jpeg = els.emitJpeg.checked;
    await persistSettings();
  });
  els.stripObi.addEventListener("change", async () => {
    state.settings.strip_obi = els.stripObi.checked;
    await persistSettings();
  });
  els.cutRatio.addEventListener("input", () => {
    els.cutLabel.textContent = `${els.cutRatio.value}%`;
  });
  els.cutRatio.addEventListener("change", async () => {
    state.settings.obi_cut_ratio = Number(els.cutRatio.value) / 100;
    await persistSettings();
  });
  els.pickTemplate.addEventListener("click", async () => {
    state.settings = await invoke("choose_template_pdf", { settings: state.settings });
    renderSettings();
  });
  els.resetTemplate.addEventListener("click", async () => {
    state.settings = await invoke("reset_template", { settings: state.settings });
    renderSettings();
  });
}

init().catch((error) => {
  document.body.innerHTML = `<pre>${String(error)}</pre>`;
});
