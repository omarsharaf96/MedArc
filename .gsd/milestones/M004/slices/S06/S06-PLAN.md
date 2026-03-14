# S06: Analytics & Outcomes Dashboard

**Goal:** The analytics dashboard renders 6 configurable KPI widget cards (visits/day, cancellation rate, units/visit, revenue/visit, net collection rate, days in A/R) plus a clinical outcomes panel (MCID achievement rate by measure, payer mix) using recharts. All values derived from live DB data with a date-range filter. Proven by rendering the dashboard against at least one month of real encounter and billing data with zero console errors.

**Demo:** SystemAdmin opens the Analytics page, selects "Last 90 days", and sees all 6 KPI cards populated with values: "12.3 visits/day", "8% cancellation rate", "4.8 units/visit", "$112 revenue/visit", "97.2% net collection rate", "22 days in A/R". The clinical outcomes panel shows a bar chart of MCID achievement rates by measure. Payer mix pie chart shows Medicare 60%, BCBS 25%, other 15%. Hovering a chart bar shows the tooltip value.

## Must-Haves

- No new migrations — S06 queries existing tables only
- `get_operational_kpis(date_range)`, `get_financial_kpis(date_range)`, `get_clinical_outcomes(date_range, provider_id?)`, `get_payer_mix(date_range)`, `get_ar_snapshot()`, `get_cancellation_rate(date_range)` Tauri commands registered
- All KPI commands return `null` for metrics where no data exists; frontend renders "No data" gracefully
- New `Analytics` RBAC resource added to `roles.rs`: SystemAdmin + Provider: full Read; BillingStaff: Read financial KPIs only (enforced at command level: financial commands check for `Analytics` resource, operational/clinical check role)
- recharts `^2.12` added to `package.json`
- `AnalyticsDashboard.tsx` renders 6 `KpiWidget` cards + `PieChart` payer mix + `BarChart` MCID achievement rates
- Date range selector (presets: Last 30d, 90d, 12mo, Calendar year, Custom) drives all queries
- MCID achievement rates calculated per measure type using M003/S02 `outcome_score_index` data
- KPI cards show period-over-period change arrow (current period vs prior equal period)
- recharts renders without console errors in the Tauri WKWebView
- All analytics commands write audit rows
- `src/types/analytics.ts` — TypeScript types for all analytics shapes
- `cargo test --lib` passes with ≥2 new analytics unit tests (net collection rate formula, MCID rate derivation with known data)
- `tsc --noEmit` exits 0

## Proof Level

- This slice proves: **contract + integration**
- Real runtime required: yes — recharts must render in the Tauri WKWebView with no console errors
- Human/UAT required: yes — dashboard values verified manually against known encounter and billing data

## Verification

```bash
# 1. Contract
cd src-tauri && cargo test --lib 2>&1 | tail -5

# 2. TypeScript contract
cd .. && npx tsc --noEmit 2>&1 | tail -5

# 3. Unit tests:
#    - net_collection_rate: charges=$1000, payments=$900, contractual_adj=$100 → rate = 1.0 (100%)
#    - mcid_rate: 4 episodes: 2 achieve MCID (change >= threshold), 2 don't → rate = 0.5 (50%)

# 4. Dashboard render (manual in Tauri dev app):
#    - Open Analytics page → all 6 KPI cards render (values or "No data")
#    - No browser console errors
#    - Payer mix pie chart renders
#    - Date range change triggers data reload
```

## Observability / Diagnostics

- Runtime signals: `write_audit_entry` for `analytics.operational_kpis`, `analytics.financial_kpis`, `analytics.clinical_outcomes`, `analytics.payer_mix`; audit details contain `date_range` only (no patient-level PHI)
- Inspection surfaces:
  - All queries are pure SQL; can be tested directly against SQLite
  - `tracing::info!` log on each command execution with query duration in ms
- Failure state: `AppError::Db` propagates from SQL errors; frontend KPI card shows error state with message; other cards continue rendering independently

## Integration Closure

- Upstream surfaces consumed:
  - `billing_index` (S01) — charges for financial KPIs
  - `payment_posting_index` (S03) — posted payments for net collection rate
  - `claim_index` (S02) — days in A/R, payer mix
  - `pt_note_index` (M003/S01) — visits per day, average visit duration
  - `appointment_index` (M001) — cancellation rate
  - `outcome_score_index` (M003/S02) — MCID achievement rates
  - `payer_config` (S02) — payer names for payer mix labels
- New wiring introduced:
  - `commands/analytics.rs` registered in `commands/mod.rs` and `lib.rs`
  - `Analytics` resource added to `roles.rs` RBAC matrix
  - Six Tauri commands in `invoke_handler!`
  - `AnalyticsDashboard.tsx` as new route target accessible from main sidebar
  - recharts added to `package.json`
- What remains: S07 MIPS dashboard reuses `AnalyticsDashboard` widget card layout

## Tasks

- [ ] **T01: Backend — analytics module and aggregate SQL queries** `est:2h`
  - Why: Retiring the recharts rendering risk requires live data from the backend. Building the SQL aggregates first ensures the frontend has real numbers to display.
  - Files: `src-tauri/src/commands/analytics.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/rbac/roles.rs`, `src-tauri/src/lib.rs`, `src/types/analytics.ts` (new), `src/lib/tauri.ts`
  - Do:
    1. Create `src-tauri/src/commands/analytics.rs` with: (a) six Tauri commands; (b) each command executes the corresponding aggregate SQL query with parameterised `start_date` / `end_date`; (c) MCID constants as module-level `const` values per measure; (d) `#[cfg(test)]` module with ≥2 unit tests using in-memory DB seeded with known data
    2. Add `Analytics` resource to `Resource` enum in `rbac/roles.rs`; assign permissions per M004 CONTEXT
    3. Add `pub mod analytics;` to `commands/mod.rs`; register commands in `lib.rs`
    4. Create `src/types/analytics.ts` with `OperationalKpis`, `FinancialKpis`, `ClinicalOutcomes`, `PayerMix`, `ArAgingBucket`, `KpiDateRange`, `McidRateByMeasure`
    5. Append analytics wrappers to `src/lib/tauri.ts` under `// M004/S06`
  - Verify: `cargo test --lib` passes with ≥2 new analytics tests; `tsc --noEmit` exits 0

- [ ] **T02: Frontend — AnalyticsDashboard with recharts** `est:3h`
  - Why: Retiring the recharts risk in Tauri WKWebView. Delivers ANLT-01 through ANLT-06.
  - Files: `src/pages/AnalyticsDashboard.tsx` (new), `src/components/analytics/KpiWidget.tsx` (new), `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`, `src/components/shell/Sidebar.tsx`, `package.json`
  - Do:
    1. Add `recharts: "^2.12"` to `package.json`
    2. Create `src/components/analytics/KpiWidget.tsx` — accepts `KpiWidgetProps`; renders title, value, unit, change arrow, mini sparkline `LineChart`
    3. Create `src/pages/AnalyticsDashboard.tsx`:
       - Date range selector at top (5 preset buttons + custom date picker)
       - 6 KPI widget cards in a 3×2 grid (operational: visits/day, cancellation rate, units/visit; financial: revenue/visit, net collection rate, days in A/R)
       - Clinical Outcomes section: recharts `BarChart` showing MCID achievement % per measure type (LEFS, DASH, NDI, Oswestry, PSFS)
       - Payer Mix section: recharts `PieChart` with legend showing payer names and percentages
       - All sections show "No data available" state when data is null/empty
       - Date range change triggers `useEffect` re-fetch of all KPI commands
    4. Add `{ page: "analytics" }` route to `RouterContext.tsx`
    5. Add ContentArea dispatch
    6. Add "Analytics" link to main sidebar (SystemAdmin + Provider)
  - Verify: `tsc --noEmit` exits 0; recharts renders without console errors in Tauri dev app; all 6 KPI cards display with real data; payer mix pie and MCID bar charts render correctly

## Files Likely Touched

- `src-tauri/src/commands/analytics.rs` — new module (T01)
- `src-tauri/src/commands/mod.rs` — `pub mod analytics` (T01)
- `src-tauri/src/rbac/roles.rs` — `Analytics` resource added (T01)
- `src-tauri/src/lib.rs` — 6 commands registered (T01)
- `src/types/analytics.ts` — new file (T01)
- `src/lib/tauri.ts` — analytics wrappers appended (T01)
- `src/pages/AnalyticsDashboard.tsx` — new page (T02)
- `src/components/analytics/KpiWidget.tsx` — new component (T02)
- `src/contexts/RouterContext.tsx` — new route (T02)
- `src/components/shell/ContentArea.tsx` — dispatch case (T02)
- `src/components/shell/Sidebar.tsx` — Analytics navigation item (T02)
- `package.json` — recharts added (T02)
