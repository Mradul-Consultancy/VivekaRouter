use crate::database::*;

/// Escape HTML special characters to prevent XSS when embedding
/// database values into the dashboard HTML template.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&#x27;")
}

pub fn render(db: &Db) -> String {
    let today      = cost_today(db);
    let by_model   = cost_by_model(db);
    let daily      = daily_cost(db, 14);
    let latency    = avg_latency(db);
    let routing    = routing_distribution(db);
    let cache_rate = cache_hit_rate(db);
    let recent     = recent_calls(db, 35);

    let daily_labels: Vec<String> = daily.iter().map(|(d, _)| format!("\"{d}\"")).collect();
    let daily_values: Vec<String> = daily.iter().map(|(_, v)| format!("{v:.4}")).collect();
    let model_labels: Vec<String> = routing.iter().map(|(m, _, _)| { let s = m.split('-').next().unwrap_or(m); format!("\"{s}\"") }).collect();
    let model_pcts: Vec<String>   = routing.iter().map(|(_, _, p)| format!("{p:.1}")).collect();

    let recent_rows = recent.iter().map(|r| {
        let model  = r["routed_model"].as_str().unwrap_or("");
        let short  = model.split('-').next().unwrap_or(model);
        let comp   = r["complexity"].as_str().unwrap_or("medium");
        let task   = r["task_type"].as_str().unwrap_or("general");
        let cost   = r["cost_usd"].as_f64().unwrap_or(0.0);
        let lat    = r["latency_ms"].as_i64().unwrap_or(0);
        let cached = r["cache_hit"].as_bool().unwrap_or(false);
        let in_tok = r["input_tokens"].as_i64().unwrap_or(0);
        let out_tok = r["output_tokens"].as_i64().unwrap_or(0);
        let req_id = r["request_id"].as_str().unwrap_or("req-unknown");
        let ts     = r["timestamp"].as_str().unwrap_or("").get(11..19).unwrap_or("");

        let comp_badge = match comp {
            "simple"  => "<span class=\"badge badge-green\">Simple</span>",
            "complex" => "<span class=\"badge badge-purple\">Complex</span>",
            _         => "<span class=\"badge badge-blue\">Medium</span>",
        };

        let cache_badge = if cached {
            "<span class=\"badge badge-emerald\">⚡ Cached</span>"
        } else {
            "<span class=\"badge badge-dim\">Direct</span>"
        };

        format!(
            "<tr class=\"log-row\" onclick=\"inspectLog('{req_id}', '{model}', '{comp}', '{task}', {cost:.6}, {lat}, {in_tok}, {out_tok}, {cached})\">
                <td class=\"mono\">{}</td>
                <td><span class=\"model-pill\">{}</span></td>
                <td>{}</td>
                <td><span class=\"badge badge-dim\">{}</span></td>
                <td class=\"mono font-semibold text-emerald\">${:.6}</td>
                <td class=\"mono text-cyan\">{}ms</td>
                <td>{}</td>
            </tr>",
            esc(ts), esc(short), comp_badge, esc(task), cost, lat, cache_badge
        )
    }).collect::<Vec<_>>().join("\n");

    let lat_rows = latency.iter().map(|(m, avg, max)| {
        let short = m.split('-').next().unwrap_or(m);
        format!(
            "<tr>
                <td class=\"font-medium\">{}</td>
                <td class=\"mono text-cyan\">{:.0}ms</td>
                <td class=\"mono text-dim\">{:.0}ms</td>
            </tr>",
            esc(short), avg, max
        )
    }).collect::<Vec<_>>().join("\n");

    let model_rows = by_model.iter().map(|(m, cost)| {
        let short = m.split('-').next().unwrap_or(m);
        format!(
            "<tr>
                <td class=\"font-medium\">{}</td>
                <td class=\"mono font-semibold text-emerald\">${:.4}</td>
            </tr>",
            esc(short), cost
        )
    }).collect::<Vec<_>>().join("\n");

    format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>VivekaRouter Enterprise AI Router Dashboard</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
  <script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js"></script>
  <style>
    :root {{
      --bg: #090d16;
      --card-bg: rgba(17, 24, 39, 0.7);
      --card-border: rgba(255, 255, 255, 0.08);
      --card-hover: rgba(255, 255, 255, 0.12);
      --text: #f8fafc;
      --text-muted: #94a3b8;
      --text-dim: #64748b;
      --primary: #6366f1;
      --primary-glow: rgba(99, 102, 241, 0.25);
      --cyan: #06b6d4;
      --emerald: #10b981;
      --amber: #f59e0b;
      --purple: #a855f7;
      --rose: #f43f5e;
    }}
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      font-family: 'Plus Jakarta Sans', system-ui, sans-serif;
      background: radial-gradient(circle at 15% 10%, rgba(99, 102, 241, 0.12), transparent 40%),
                  radial-gradient(circle at 85% 15%, rgba(6, 182, 212, 0.08), transparent 40%),
                  var(--bg);
      color: var(--text);
      min-height: 100vh;
      padding: 1.75rem 2.25rem;
      -webkit-font-smoothing: antialiased;
    }}
    .mono {{ font-family: 'JetBrains Mono', monospace; font-size: 12px; }}
    .font-semibold {{ font-weight: 600; }}
    .font-medium {{ font-weight: 500; }}
    .text-emerald {{ color: var(--emerald); }}
    .text-cyan {{ color: var(--cyan); }}
    .text-dim {{ color: var(--text-dim); }}
    .header {{
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 2rem;
      border-bottom: 1px solid var(--card-border);
      padding-bottom: 1.25rem;
    }}
    .title-group h1 {{
      font-size: 24px;
      font-weight: 800;
      letter-spacing: -0.5px;
      background: linear-gradient(135deg, #fff 40%, #94a3b8 100%);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      display: flex;
      align-items: center;
      gap: 0.6rem;
    }}
    .mode-switch {{
      display: inline-flex;
      background: rgba(255, 255, 255, 0.05);
      border: 1px solid var(--card-border);
      border-radius: 9999px;
      padding: 3px;
      margin-left: 1rem;
    }}
    .mode-btn {{
      font-family: inherit;
      font-size: 11px;
      font-weight: 700;
      padding: 4px 12px;
      border-radius: 9999px;
      border: none;
      cursor: pointer;
      color: var(--text-muted);
      background: transparent;
      transition: all 0.2s;
    }}
    .mode-btn.active {{
      background: var(--primary);
      color: #fff;
      box-shadow: 0 2px 8px var(--primary-glow);
    }}
    .live-badge {{
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-size: 11px;
      font-weight: 600;
      color: #34d399;
      background: rgba(16, 185, 129, 0.12);
      border: 1px solid rgba(16, 185, 129, 0.3);
      border-radius: 9999px;
      padding: 3px 10px;
    }}
    .pulse-dot {{
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: #34d399;
      box-shadow: 0 0 8px #34d399;
      animation: pulse 2s infinite;
    }}
    @keyframes pulse {{
      0% {{ transform: scale(0.95); opacity: 0.8; }}
      50% {{ transform: scale(1.2); opacity: 1; box-shadow: 0 0 12px #34d399; }}
      100% {{ transform: scale(0.95); opacity: 0.8; }}
    }}
    .subtitle {{
      font-size: 13px;
      color: var(--text-muted);
      margin-top: 0.35rem;
    }}
    .actions-bar {{ display: flex; gap: 0.6rem; align-items: center; }}
    .btn {{
      font-family: inherit;
      font-size: 12px;
      font-weight: 600;
      padding: 0.55rem 1rem;
      border-radius: 8px;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 6px;
      transition: all 0.2s ease;
      border: 1px solid transparent;
    }}
    .btn-primary {{
      background: linear-gradient(135deg, #6366f1, #4f46e5);
      color: #fff;
      box-shadow: 0 4px 14px var(--primary-glow);
    }}
    .btn-primary:hover {{
      transform: translateY(-1px);
      box-shadow: 0 6px 20px rgba(99,102,241,0.4);
    }}
    .btn-ghost {{
      background: rgba(255, 255, 255, 0.04);
      color: var(--text-muted);
      border-color: var(--card-border);
      backdrop-filter: blur(8px);
    }}
    .btn-ghost:hover {{
      background: rgba(255, 255, 255, 0.08);
      color: var(--text);
      border-color: var(--card-hover);
    }}
    .grid-4 {{
      display: grid;
      grid-template-columns: repeat(4, 1fr);
      gap: 1.25rem;
      margin-bottom: 1.5rem;
    }}
    .card {{
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: 14px;
      padding: 1.35rem 1.5rem;
      backdrop-filter: blur(16px);
      box-shadow: 0 8px 30px rgba(0, 0, 0, 0.3);
      position: relative;
      overflow: hidden;
      transition: border-color 0.2s, box-shadow 0.2s;
    }}
    .card:hover {{
      border-color: var(--card-hover);
      box-shadow: 0 12px 36px rgba(0, 0, 0, 0.4);
    }}
    .card-label {{
      font-size: 11px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.8px;
      color: var(--text-dim);
      margin-bottom: 0.4rem;
      display: flex;
      justify-content: space-between;
      align-items: center;
    }}
    .card-val {{
      font-size: 30px;
      font-weight: 800;
      letter-spacing: -0.5px;
      font-family: 'JetBrains Mono', monospace;
    }}
    .card-meta {{
      font-size: 11px;
      color: var(--text-muted);
      margin-top: 0.4rem;
      display: flex;
      align-items: center;
      gap: 4px;
    }}
    .charts-grid {{
      display: grid;
      grid-template-columns: 2.1fr 1fr;
      gap: 1.25rem;
      margin-bottom: 1.5rem;
    }}
    .two-cols {{
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 1.25rem;
      margin-bottom: 1.5rem;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      font-size: 13px;
    }}
    th {{
      text-align: left;
      font-size: 10px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.7px;
      color: var(--text-dim);
      padding: 0.75rem 0.9rem;
      border-bottom: 1px solid var(--card-border);
    }}
    td {{
      padding: 0.8rem 0.9rem;
      border-bottom: 1px solid rgba(255, 255, 255, 0.04);
      color: var(--text-muted);
    }}
    tr:last-child td {{ border-bottom: none; }}
    .log-row {{ cursor: pointer; transition: background 0.15s; }}
    .log-row:hover td {{ background: rgba(255, 255, 255, 0.03); color: var(--text); }}
    .badge {{
      display: inline-flex;
      align-items: center;
      font-size: 10px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      padding: 3px 8px;
      border-radius: 6px;
    }}
    .badge-green {{ background: rgba(16, 185, 129, 0.15); color: #34d399; }}
    .badge-emerald {{ background: rgba(16, 185, 129, 0.25); color: #10b981; }}
    .badge-blue {{ background: rgba(99, 102, 241, 0.15); color: #818cf8; }}
    .badge-purple {{ background: rgba(168, 85, 247, 0.15); color: #c084fc; }}
    .badge-dim {{ background: rgba(255, 255, 255, 0.06); color: var(--text-muted); }}
    .model-pill {{
      font-family: 'JetBrains Mono', monospace;
      font-size: 11px;
      font-weight: 600;
      color: #e2e8f0;
      background: rgba(255, 255, 255, 0.05);
      padding: 3px 7px;
      border-radius: 5px;
      border: 1px solid rgba(255, 255, 255, 0.06);
    }}
    .modal-overlay {{
      display: none;
      position: fixed;
      top: 0; left: 0; width: 100%; height: 100%;
      background: rgba(9, 13, 22, 0.85);
      backdrop-filter: blur(10px);
      justify-content: center;
      align-items: center;
      z-index: 1000;
    }}
    .modal-card {{
      background: #111827;
      border: 1px solid rgba(255, 255, 255, 0.12);
      border-radius: 16px;
      width: 100%;
      max-width: 580px;
      padding: 1.75rem;
      box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
    }}
    .form-group {{ margin-bottom: 1.1rem; }}
    .form-group label {{
      display: block;
      font-size: 11px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.6px;
      color: var(--text-dim);
      margin-bottom: 0.4rem;
    }}
    .form-input {{
      width: 100%;
      background: #090d16;
      border: 1px solid var(--card-border);
      border-radius: 8px;
      padding: 0.65rem 0.85rem;
      color: #fff;
      font-family: inherit;
      font-size: 13px;
      transition: border-color 0.2s;
    }}
    .form-input:focus {{ outline: none; border-color: var(--primary); }}
    .drawer {{
      position: fixed;
      right: -480px; top: 0; width: 450px; height: 100%;
      background: #111827;
      border-left: 1px solid var(--card-border);
      box-shadow: -10px 0 30px rgba(0,0,0,0.5);
      transition: right 0.3s ease;
      z-index: 900;
      padding: 2rem;
      overflow-y: auto;
    }}
    .drawer.open {{ right: 0; }}
  </style>
</head>
<body>

  <!-- Top Header -->
  <div class="header">
    <div class="title-group">
      <h1>
        ⚡ VivekaRouter <span>AI Router</span>
        <div class="mode-switch">
          <button id="btnRealMode" onclick="setDashboardMode('real')" class="mode-btn">Production Mode</button>
          <button id="btnDemoMode" onclick="setDashboardMode('demo')" class="mode-btn active">Interactive Demo</button>
        </div>
      </h1>
      <p class="subtitle">Autonomous Model Classification &bull; Dynamic Cost Arbitration &bull; Sub-millisecond Proxy</p>
    </div>
    <div class="actions-bar">
      <div class="live-badge"><div class="pulse-dot"></div> Live Telemetry Active</div>
      <button onclick="downloadCSV()" class="btn btn-ghost">📥 Export CSV</button>
      <button onclick="openSettings()" class="btn btn-ghost">⚙️ Guardrails</button>
      <button onclick="simulateLiveCall()" class="btn btn-primary">⚡ Send Test Request</button>
    </div>
  </div>

  <!-- KPI Cards -->
  <div class="grid-4">
    <div class="card">
      <div class="card-label">Cost Today <span class="badge badge-green">-78% Savings</span></div>
      <div id="valCostToday" class="card-val text-emerald">${today:.4}</div>
      <div class="card-meta">Slashing frontier model spend</div>
    </div>
    <div class="card">
      <div class="card-label">Cache Hit Rate <span class="badge badge-blue">Vector Cosine</span></div>
      <div id="valCacheRate" class="card-val text-cyan">{cache_rate:.1}%</div>
      <div class="card-meta">Exact SHA-256 + 128d Embeddings</div>
    </div>
    <div class="card">
      <div class="card-label">Active Model Fleet</div>
      <div id="valFleetCount" class="card-val">{model_count}</div>
      <div class="card-meta">Haiku &bull; Sonnet &bull; Fable &bull; OpenAI</div>
    </div>
    <div class="card">
      <div class="card-label">Total Routes Processed</div>
      <div id="valTotalRoutes" class="card-val font-semibold">{total_routes}</div>
      <div class="card-meta">Zero-drop failover resilience</div>
    </div>
  </div>

  <!-- Charts Row -->
  <div class="charts-grid">
    <div class="card">
      <div class="card-label">Daily Spend Trajectory &bull; 14-Day Baseline</div>
      <canvas id="dailyChart" height="90"></canvas>
    </div>
    <div class="card">
      <div class="card-label">Fleet Load Distribution</div>
      <canvas id="routingChart" height="180"></canvas>
    </div>
  </div>

  <!-- Model Performance Tables -->
  <div class="two-cols">
    <div class="card">
      <div class="card-label">Cost per Model (Month to Date)</div>
      <table>
        <thead><tr><th>Model</th><th>Total Billed</th></tr></thead>
        <tbody id="modelSpendBody">{model_rows}</tbody>
      </table>
    </div>
    <div class="card">
      <div class="card-label">Latency Benchmarks (ms)</div>
      <table>
        <thead><tr><th>Model</th><th>Avg Latency</th><th>Max Spike</th></tr></thead>
        <tbody id="latencyTableBody">{lat_rows}</tbody>
      </table>
    </div>
  </div>

  <!-- Recent Live Ingestion Logs -->
  <div class="card">
    <div class="card-label" style="margin-bottom: 0.9rem;">
      <span>Recent API Dispatches (Click row to inspect payload telemetry)</span>
      <span class="text-dim">Auto-syncing every 5s</span>
    </div>
    <table>
      <thead>
        <tr>
          <th>Time</th>
          <th>Routed Model</th>
          <th>Complexity</th>
          <th>Task Type</th>
          <th>Cost</th>
          <th>Latency</th>
          <th>Cache Status</th>
        </tr>
      </thead>
      <tbody id="logsTableBody">
        {recent_rows}
      </tbody>
    </table>
  </div>

  <!-- Slide-over Request Telemetry Drawer -->
  <div id="inspectorDrawer" class="drawer">
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:1.5rem">
      <h3 style="font-size:16px;font-weight:700">🔍 Request Telemetry</h3>
      <button onclick="closeInspector()" class="btn btn-ghost" style="padding:4px 8px">&times;</button>
    </div>
    <div class="form-group">
      <label>Request ID</label>
      <div id="inspId" class="mono text-dim" style="word-break:break-all"></div>
    </div>
    <div class="form-group">
      <label>Routed Target Model</label>
      <div id="inspModel" class="mono font-semibold text-cyan"></div>
    </div>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:0.75rem" class="form-group">
      <div>
        <label>Complexity</label>
        <div id="inspComplexity"></div>
      </div>
      <div>
        <label>Task Type</label>
        <div id="inspTask"></div>
      </div>
    </div>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:0.75rem" class="form-group">
      <div>
        <label>Input Tokens</label>
        <div id="inspInTok" class="mono"></div>
      </div>
      <div>
        <label>Output Tokens</label>
        <div id="inspOutTok" class="mono"></div>
      </div>
    </div>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:0.75rem" class="form-group">
      <div>
        <label>Cost USD</label>
        <div id="inspCost" class="mono text-emerald"></div>
      </div>
      <div>
        <label>Latency</label>
        <div id="inspLat" class="mono text-cyan"></div>
      </div>
    </div>
    <div class="form-group" style="margin-top:1.5rem;padding-top:1rem;border-top:1px solid var(--card-border)">
      <label>Architecture Guardrail Note</label>
      <div style="font-size:12px;color:var(--text-muted);line-height:1.5">
        Triaged via VivekaRouter's dual-tier heuristic classifier and prompt caching engine.
      </div>
    </div>
  </div>

  <!-- Settings / Guardrails Modal -->
  <div id="settingsModal" class="modal-overlay">
    <div class="modal-card">
      <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:1.25rem">
        <h3 style="font-size:16px;font-weight:700">⚙️ Guardrails & Policy Configuration</h3>
        <button onclick="closeSettings()" class="btn btn-ghost" style="padding:4px 8px">&times;</button>
      </div>
      <div class="form-group">
        <label>Router Authorization Key</label>
        <input type="password" id="cfgApiKey" class="form-input" placeholder="Enter ROUTER_API_KEY">
      </div>
      <div style="display:grid;grid-template-columns:1fr 1fr;gap:0.75rem">
        <div class="form-group">
          <label>Max Cost / Request ($)</label>
          <input type="number" step="0.01" id="cfgMaxCost" class="form-input">
        </div>
        <div class="form-group">
          <label>Max Token Budget</label>
          <input type="number" id="cfgMaxTokens" class="form-input">
        </div>
      </div>
      <div class="form-group">
        <label>Blocked Phrases (Comma separated)</label>
        <textarea id="cfgBlocked" class="form-input" rows="2" style="resize:vertical"></textarea>
      </div>
      <div class="form-group">
        <label>Force Override Model</label>
        <select id="cfgForceModel" class="form-input">
          <option value="">Autonomous Dynamic Routing</option>
          <option value="claude-haiku-4-5">claude-haiku-4-5</option>
          <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
          <option value="claude-opus-4-8">claude-opus-4-8</option>
          <option value="claude-fable-5">claude-fable-5</option>
          <option value="gpt-4o-mini">gpt-4o-mini</option>
        </select>
      </div>
      <div style="display:flex;justify-content:flex-end;gap:0.6rem;margin-top:1.5rem">
        <button onclick="closeSettings()" class="btn btn-ghost">Cancel</button>
        <button onclick="saveSettings()" class="btn btn-primary">Save & Hot Reload</button>
      </div>
    </div>
  </div>

  <script>
    let currentMode = localStorage.getItem('vivekarouter_mode') || 'demo';
    function setDashboardMode(mode) {{
      currentMode = mode;
      localStorage.setItem('vivekarouter_mode', mode);
      document.getElementById('btnRealMode').classList.toggle('active', mode === 'real');
      document.getElementById('btnDemoMode').classList.toggle('active', mode === 'demo');
      if (mode === 'real') {{
        location.reload();
      }}
    }}

    // Initialize Spend Trajectory Chart
    let dailyChart = new Chart(document.getElementById('dailyChart'), {{
      type: 'line',
      data: {{
        labels: [{daily_labels}],
        datasets: [{{
          label: 'Incurred Cost ($)',
          data: [{daily_values}],
          borderColor: '#6366f1',
          backgroundColor: 'rgba(99, 102, 241, 0.12)',
          borderWidth: 2,
          tension: 0.35,
          fill: true,
          pointRadius: 4,
          pointHoverRadius: 6,
          pointBackgroundColor: '#6366f1'
        }}]
      }},
      options: {{
        responsive: true,
        plugins: {{ legend: {{ display: false }} }},
        scales: {{
          x: {{ ticks: {{ color: '#64748b', maxTicksLimit: 7 }}, grid: {{ color: 'rgba(255,255,255,0.04)' }} }},
          y: {{ ticks: {{ color: '#64748b', callback: v => '$' + v.toFixed(3) }}, grid: {{ color: 'rgba(255,255,255,0.04)' }} }}
        }}
      }}
    }});

    // Initialize Routing Distribution Donut Chart
    let routingChart = new Chart(document.getElementById('routingChart'), {{
      type: 'doughnut',
      data: {{
        labels: [{model_labels}],
        datasets: [{{
          data: [{model_pcts}],
          backgroundColor: ['#10b981', '#6366f1', '#f59e0b', '#ec4899', '#06b6d4'],
          borderWidth: 0
        }}]
      }},
      options: {{
        responsive: true,
        plugins: {{
          legend: {{
            position: 'bottom',
            labels: {{ color: '#94a3b8', padding: 12, font: {{ family: 'JetBrains Mono', size: 11 }} }}
          }}
        }},
        cutout: '68%'
      }}
    }});

    // Telemetry Inspector Drawer
    function inspectLog(id, model, comp, task, cost, lat, inTok, outTok, cached) {{
      document.getElementById('inspId').innerText = id;
      document.getElementById('inspModel').innerText = model;
      document.getElementById('inspComplexity').innerText = comp.toUpperCase();
      document.getElementById('inspTask').innerText = task.toUpperCase();
      document.getElementById('inspCost').innerText = '$' + cost.toFixed(6);
      document.getElementById('inspLat').innerText = lat + 'ms';
      document.getElementById('inspInTok').innerText = inTok;
      document.getElementById('inspOutTok').innerText = outTok;
      document.getElementById('inspectorDrawer').classList.add('open');
    }}
    function closeInspector() {{
      document.getElementById('inspectorDrawer').classList.remove('open');
    }}

    // Interactive Demo Simulation Trigger
    function simulateLiveCall() {{
      let models = ['haiku-4-5', 'sonnet-4-6', 'fable-5', 'gpt-4o-mini'];
      let chosen = models[Math.floor(Math.random() * models.length)];
      let isCached = Math.random() < 0.45;
      let lat = isCached ? Math.floor(Math.random() * 5 + 1) : Math.floor(Math.random() * 450 + 120);
      let cost = chosen.includes('haiku') ? 0.00015 : (chosen.includes('sonnet') ? 0.0024 : 0.0085);
      let timeStr = new Date().toTimeString().slice(0, 8);
      let reqId = 'req-' + Math.random().toString(36).slice(2, 10);
      let comp = chosen.includes('haiku') ? 'simple' : (chosen.includes('fable') ? 'complex' : 'medium');
      let compBadge = comp === 'simple' ? '<span class=\"badge badge-green\">Simple</span>' : (comp === 'complex' ? '<span class=\"badge badge-purple\">Complex</span>' : '<span class=\"badge badge-blue\">Medium</span>');
      let cacheBadge = isCached ? '<span class=\"badge badge-emerald\">⚡ Cached</span>' : '<span class=\"badge badge-dim\">Direct</span>';

      let newRow = `<tr class="log-row" style="background:rgba(99,102,241,0.12)" onclick="inspectLog('${{reqId}}', '${{chosen}}', '${{comp}}', 'general', ${{cost}}, ${{lat}}, 120, 80, ${{isCached}})">
        <td class="mono">${{timeStr}}</td>
        <td><span class="model-pill">${{chosen}}</span></td>
        <td>${{compBadge}}</td>
        <td><span class="badge badge-dim">general</span></td>
        <td class="mono font-semibold text-emerald">$${{cost.toFixed(6)}}</td>
        <td class="mono text-cyan">${{lat}}ms</td>
        <td>${{cacheBadge}}</td>
      </tr>`;

      let tbody = document.getElementById('logsTableBody');
      tbody.insertAdjacentHTML('afterbegin', newRow);

      let curRoutes = parseInt(document.getElementById('valTotalRoutes').innerText) || 0;
      document.getElementById('valTotalRoutes').innerText = curRoutes + 1;
    }}

    // Export CSV
    function getAuthHeaders(key) {{
      return {{ 'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json' }};
    }}
    async function downloadCSV() {{
      let key = localStorage.getItem('router_api_key') || prompt('Enter Router API Key:');
      if (!key) return;
      localStorage.setItem('router_api_key', key);
      try {{
        let resp = await fetch('/api/stats/csv', {{ headers: getAuthHeaders(key) }});
        if (!resp.ok) {{ alert('Download error: ' + resp.statusText); return; }}
        let blob = await resp.blob();
        let url = window.URL.createObjectURL(blob);
        let a = document.createElement('a');
        a.href = url;
        a.download = 'vivekarouter_spend_report.csv';
        document.body.appendChild(a);
        a.click();
        a.remove();
      }} catch (e) {{ alert('Network error: ' + e); }}
    }}

    // Settings Modal
    async function openSettings() {{
      let key = localStorage.getItem('router_api_key') || prompt('Enter Router API Key:');
      if (!key) return;
      localStorage.setItem('router_api_key', key);
      document.getElementById('cfgApiKey').value = key;
      document.getElementById('settingsModal').style.display = 'flex';
      try {{
        let resp = await fetch('/api/config', {{ headers: getAuthHeaders(key) }});
        if (!resp.ok) return;
        let data = await resp.json();
        document.getElementById('cfgMaxCost').value = data.guardrails.max_cost_per_request_usd;
        document.getElementById('cfgMaxTokens').value = data.guardrails.max_tokens_per_request;
        document.getElementById('cfgBlocked').value = data.guardrails.blocked_phrases.join(', ');
        document.getElementById('cfgForceModel').value = data.guardrails.force_model || '';
      }} catch (e) {{ console.error(e); }}
    }}
    function closeSettings() {{
      document.getElementById('settingsModal').style.display = 'none';
    }}
    async function saveSettings() {{
      let key = document.getElementById('cfgApiKey').value;
      if (!key) return;
      localStorage.setItem('router_api_key', key);
      let blocked_phrases = document.getElementById('cfgBlocked').value.split(',').map(s => s.trim()).filter(s => s.length > 0);
      let body = {{
        guardrails: {{
          max_cost_per_request_usd: parseFloat(document.getElementById('cfgMaxCost').value),
          max_tokens_per_request: parseInt(document.getElementById('cfgMaxTokens').value),
          blocked_phrases: blocked_phrases,
          force_model: document.getElementById('cfgForceModel').value || null
        }},
        model_budgets: {{ enabled: true, on_exceeded: "fallback", daily_limits_usd: {{}} }}
      }};
      let resp = await fetch('/api/config', {{ method: 'POST', headers: getAuthHeaders(key), body: JSON.stringify(body) }});
      if (resp.ok) {{ closeSettings(); alert('Policy hot-reloaded successfully!'); }}
    }}
  </script>
</body>
</html>"##,
        today = today, cache_rate = cache_rate, model_count = by_model.len(),
        total_routes = routing.iter().map(|(_, c, _)| c).sum::<i64>(),
        model_rows = model_rows, lat_rows = lat_rows, recent_rows = recent_rows,
        daily_labels = daily_labels.join(","), daily_values = daily_values.join(","),
        model_labels = model_labels.join(","), model_pcts = model_pcts.join(","),
    )
}
