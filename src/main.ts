import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

// ---- Types -----------------------------------------------------------------

interface PlatformStat {
  platform: string;
  total_requests: number;
  total_tokens: number;
  total_actual_cost: number;
  today_requests: number;
  today_tokens: number;
  today_actual_cost: number;
}

interface Stats {
  total_api_keys: number;
  active_api_keys: number;
  total_requests: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_creation_tokens: number;
  total_cache_read_tokens: number;
  total_tokens: number;
  total_cost: number;
  total_actual_cost: number;
  today_requests: number;
  today_input_tokens: number;
  today_output_tokens: number;
  today_cache_creation_tokens: number;
  today_cache_read_tokens: number;
  today_tokens: number;
  today_cost: number;
  today_actual_cost: number;
  average_duration_ms: number;
  rpm: number;
  tpm: number;
  by_platform: PlatformStat[];
}

interface GroupInfo {
  id: number;
  name: string;
  description?: string;
  platform: string;
  status: string;
  subscription_type?: string;
  daily_limit_usd?: number;
  weekly_limit_usd?: number;
  monthly_limit_usd?: number;
}

interface Subscription {
  id: number;
  starts_at: string;
  expires_at: string;
  status: string;
  daily_usage_usd?: number;
  weekly_usage_usd?: number;
  monthly_usage_usd?: number;
  group?: GroupInfo;
}

// ---- Formatters -----------------------------------------------------------
const money = (n: number) =>
  "$" +
  n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

const compact = (n: number) =>
  n.toLocaleString("en-US", { notation: "compact", maximumFractionDigits: 1 });

const moneyShort = (n: number) =>
  n < 1000
    ? "$" + Math.round(n)
    : "$" +
      n.toLocaleString("en-US", {
        notation: "compact",
        maximumFractionDigits: 1,
      });

const duration = (ms: number) => {
  const s = ms / 1000;
  if (s < 60) return s.toFixed(1) + "s";
  const m = Math.floor(s / 60);
  return m + "m" + Math.round(s % 60) + "s";
};

const time = () =>
  new Date().toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

const fmtShortDate = (iso: string) => {
  const d = new Date(iso);
  return d.toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" });
};

function daysUntil(iso: string): number {
  const now = Date.now();
  const then = new Date(iso).getTime();
  return Math.ceil((then - now) / 86_400_000);
}

/// Resize the popup height to match the rendered content.
async function fitHeight() {
  const el = document.getElementById("view-dash");
  if (!el) return;
  const pad =
    parseFloat(getComputedStyle(document.body).paddingTop) +
    parseFloat(getComputedStyle(document.body).paddingBottom);
  const h = Math.ceil(el.getBoundingClientRect().height + pad);
  try {
    await invoke("fit_height", { height: h });
  } catch {
    // non-critical
  }
}

// ---- DOM helpers ----------------------------------------------------------
const $ = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T;

function show(view: "loading" | "login" | "dash") {
  $("view-loading").classList.toggle("hidden", view !== "loading");
  $("view-login").classList.toggle("hidden", view !== "login");
  $("view-dash").classList.toggle("hidden", view !== "dash");
  invoke("set_auto_hide", { enabled: view !== "login" });
}

// ---- Render: stats dashboard ----------------------------------------------
function renderStats(s: Stats) {
  // Today
  $("today-cost").textContent = money(s.today_actual_cost);
  $("today-req").textContent = compact(s.today_requests);
  $("rpm").textContent = compact(s.rpm);
  $("tpm").textContent = compact(s.tpm);
  $("avg-dur").textContent = duration(s.average_duration_ms);

  // Total
  $("total-cost").textContent = money(s.total_actual_cost);
  $("total-tokens").textContent = compact(s.total_tokens);
  $("total-req").textContent = compact(s.total_requests);

  // Platforms (by total_actual_cost desc)
  const totalCost = s.total_actual_cost || 1;
  const plats = [...(s.by_platform || [])].sort(
    (a, b) => b.total_actual_cost - a.total_actual_cost
  );
  const wrap = $("platforms");
  wrap.innerHTML = "";
  for (const p of plats) {
    const pct = Math.round((p.total_actual_cost / totalCost) * 100);
    const row = document.createElement("div");
    row.className = "platform";

    const head = document.createElement("div");
    head.className = "platform-row";
    const name = document.createElement("span");
    name.className = "platform-name";
    name.textContent = p.platform;
    const cost = document.createElement("span");
    cost.className = "platform-cost";
    cost.textContent = `${moneyShort(p.total_actual_cost)} · ${pct}%`;
    head.append(name, cost);

    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("i");
    fill.style.width = `${pct}%`;
    bar.appendChild(fill);

    row.append(head, bar);
    wrap.appendChild(row);
  }
}

// ---- Render: subscriptions ------------------------------------------------
function renderSubs(subs: Subscription[]) {
  const wrap = $("sub-cards");
  wrap.innerHTML = "";

  if (subs.length === 0) {
    wrap.innerHTML = '<p class="muted" id="sub-empty">暂无订阅</p>';
    return;
  }

  for (const sub of subs) {
    const g = sub.group;
    const name = g?.name ?? "订阅 #" + sub.id;
    const platform = g?.platform ?? "";
    const desc = g?.description ?? "";
    const status = sub.status;

    const daily = sub.daily_usage_usd ?? 0;
    const weekly = sub.weekly_usage_usd ?? 0;
    const monthly = sub.monthly_usage_usd ?? 0;
    const dailyLimit = g?.daily_limit_usd;
    const weeklyLimit = g?.weekly_limit_usd;
    const monthlyLimit = g?.monthly_limit_usd;

    const card = document.createElement("div");
    card.className = "sub-card";

    // Header
    const head = document.createElement("div");
    head.className = "sub-head";

    const nameSpan = document.createElement("span");
    nameSpan.className = "sub-name";
    nameSpan.textContent = name;
    if (platform) {
      const plat = document.createElement("span");
      plat.className = "sub-platform";
      plat.textContent = platform;
      nameSpan.append(plat);
    }

    const statusBadge = document.createElement("span");
    statusBadge.className = "sub-status " + status;
    statusBadge.textContent = status === "active" ? "● 启用" : status;

    head.append(nameSpan, statusBadge);
    card.appendChild(head);

    // Description
    if (desc) {
      const descEl = document.createElement("p");
      descEl.className = "sub-desc";
      descEl.textContent = desc;
      card.appendChild(descEl);
    }

    // Usage grid
    const grid = document.createElement("div");
    grid.className = "sub-usage-grid";

    const makeCell = (
      label: string,
      used: number,
      limit: number | undefined,
    ) => {
      const cell = document.createElement("div");
      cell.className = "usage-cell";

      const lbl = document.createElement("div");
      lbl.className = "label";
      lbl.textContent = label;
      cell.appendChild(lbl);

      const amt = document.createElement("div");
      amt.className = "usage-amount";
      amt.textContent = money(used);
      cell.appendChild(amt);

      if (limit != null && limit > 0) {
        const lim = document.createElement("div");
        lim.className = "usage-limit";
        lim.textContent = "上限 " + money(limit);
        cell.appendChild(lim);

        const pct = Math.min(used / limit, 1);
        const barWrap = document.createElement("div");
        barWrap.className = "usage-bar-wrap";
        const bar = document.createElement("div");
        bar.className = "usage-bar";
        if (pct >= 0.9) bar.classList.add("danger");
        else if (pct >= 0.75) bar.classList.add("warn");
        else bar.classList.add("normal");
        bar.style.width = (pct * 100).toFixed(1) + "%";
        barWrap.appendChild(bar);
        cell.appendChild(barWrap);
      }

      return cell;
    };

    grid.append(
      makeCell("日消耗", daily, dailyLimit),
      makeCell("周消耗", weekly, weeklyLimit),
      makeCell("月消耗", monthly, monthlyLimit),
    );
    card.appendChild(grid);

    // Footer
    const meta = document.createElement("div");
    meta.className = "sub-meta";

    const left = document.createElement("span");
    const remain = daysUntil(sub.expires_at);
    if (remain > 0) {
      left.textContent = `剩余 ${remain} 天`;
    } else if (remain === 0) {
      left.textContent = "今日到期";
    } else {
      left.textContent = "已过期";
    }

    const right = document.createElement("span");
    right.className = "sub-expires";
    right.textContent = "到期 " + fmtShortDate(sub.expires_at);

    meta.append(left, right);
    card.appendChild(meta);

    wrap.appendChild(card);
  }
}

// ---- Combined render ------------------------------------------------------
type DashboardData = { stats: Stats; subs: Subscription[] };

function renderAll(data: DashboardData) {
  renderStats(data.stats);
  renderSubs(data.subs);

  // Update tray title with today's cost from stats
  updateTrayTitle(data.stats.today_actual_cost);

  $("updated-at").textContent = "更新于 " + time();
  fitHeight();
}

async function updateTrayTitle(cost: number) {
  const title = "\u2004$" + cost.toFixed(2);
  try {
    await invoke("set_tray_title", { title });
  } catch {
    // ignore
  }
}

// ---- Data fetch -----------------------------------------------------------
let refreshing = false;
async function refresh() {
  if (refreshing) return;
  refreshing = true;
  $("refresh-btn")?.classList.add("spinning");
  try {
    const [stats, subs] = await Promise.all([
      invoke<Stats>("fetch_stats"),
      invoke<Subscription[]>("fetch_subscriptions"),
    ]);
    renderAll({ stats, subs });
  } catch (e) {
    const msg = String(e);
    if (msg.includes("not logged in") || msg.includes("401")) {
      show("login");
    } else {
      $("updated-at").textContent = "出错: " + msg.slice(0, 40);
    }
  } finally {
    refreshing = false;
    $("refresh-btn")?.classList.remove("spinning");
  }
}

// ---- Login ----------------------------------------------------------------
async function handleLogin(ev: SubmitEvent) {
  ev.preventDefault();
  const email = ($("email") as HTMLInputElement).value.trim();
  const password = ($("password") as HTMLInputElement).value;
  const errEl = $("login-error");
  const btn = $("login-btn") as HTMLButtonElement;
  errEl.classList.add("hidden");
  btn.disabled = true;
  btn.textContent = "登录中…";
  try {
    await invoke("login", { email, password });
    show("dash");
    await refresh();
    startTimer();
  } catch (e) {
    errEl.textContent = "登录失败：" + String(e);
    errEl.classList.remove("hidden");
  } finally {
    btn.disabled = false;
    btn.textContent = "登录";
  }
}

// ---- Timer ----------------------------------------------------------------
let timer: number | undefined;
function startTimer() {
  stopTimer();
  const ms = Number(($("interval") as HTMLSelectElement).value);
  timer = window.setInterval(refresh, ms);
}
function stopTimer() {
  if (timer) {
    clearInterval(timer);
    timer = undefined;
  }
}

// ---- Boot -----------------------------------------------------------------
async function boot() {
  // Wire up events
  $("login-form").addEventListener("submit", handleLogin);
  $("refresh-btn").addEventListener("click", refresh);
  $("logout-btn").addEventListener("click", async () => {
    stopTimer();
    await invoke("logout");
    show("login");
  });
  document.querySelectorAll(".quit").forEach((btn) => {
    btn.addEventListener("click", () => invoke("quit_app"));
  });
  $("interval").addEventListener("change", () => {
    startTimer();
    refresh();
  });

  try {
    const win = getCurrentWindow();
    win.onFocusChanged(({ payload: focused }) => {
      if (focused) refresh();
    });
  } catch {
    // non-critical
  }

  let loggedIn = false;
  try {
    loggedIn = await invoke<boolean>("is_logged_in");
  } catch {
    // Keychain access or Tauri bridge failed — show login form anyway.
  }
  if (loggedIn) {
    show("dash");
    await refresh();
    startTimer();
  } else {
    show("login");
  }
}

boot();
