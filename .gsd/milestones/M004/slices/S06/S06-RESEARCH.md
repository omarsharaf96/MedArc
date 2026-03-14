# S06: Analytics & Outcomes Dashboard — Research

**Date:** 2026-03-14

## Summary

S06 adds an analytics dashboard with 6 operational/financial KPI widgets and a clinical outcomes panel. All data comes from existing tables (`billing_index`, `payment_posting_index`, `pt_note_index`, `appointment_index`, `outcome_score_index`) via aggregate SQL queries — no new migrations required.

The primary technical question is chart rendering. `recharts` is the planned library (M004 CONTEXT). It is a mature React charting library built on D3, well-suited to Tauri webview, and has no SSR concerns in a desktop app. The risk (from the ROADMAP) is confirming recharts mounts correctly in the Tauri WKWebView context without console errors — this is retired in T01.

The secondary challenge is query performance. Aggregate SQL over `billing_index` and `payment_posting_index` for 12 months of encounter data must complete in < 2 seconds. On a typical solo PT practice (1,500 encounters/year), this is trivially fast with proper indexes. The indexes from S01–S03 migrations are already designed for this access pattern.

**Confidence: HIGH** — All KPI calculations are standard PT practice management metrics with well-established formulas. Recharts integration in Tauri is confirmed by the ecosystem (many Tauri apps use it). The aggregate SQL is straightforward.

## Recommendation

- One `commands/analytics.rs` module with 6 pure SQL query commands
- No new migrations — read-only queries against existing tables
- recharts added to `package.json`; import dynamically is not needed in Tauri (no SSR)
- `AnalyticsDashboard.tsx` with 6 configurable widget cards plus payer mix pie chart and MCID trend line

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Chart rendering | recharts `^2.12` (planned in M004 CONTEXT) | Production-quality React charting; `LineChart`, `BarChart`, `PieChart` all needed |
| Date range filtering | Existing `date_range: Option<(String, String)>` pattern | All KPI queries accept optional `start_date`/`end_date` ISO-8601 strings |
| RBAC | `Analytics` resource (from M004 CONTEXT) | SystemAdmin + Provider: full read; BillingStaff: financial KPIs only |
| Audit log | `write_audit_entry` | Analytics reads are ePHI aggregate access |

## PT Practice KPIs

### Operational KPIs

| KPI | Formula | Data Source | Normal Range |
|-----|---------|-------------|-------------|
| Visits per day | `COUNT(encounters) / working_days` | `pt_note_index` (status = 'signed' or 'locked') | Solo PT: 8–16/day |
| Cancellation rate | `cancelled_appointments / scheduled_appointments` | `appointment_index` | Target: < 15% |
| Units per visit | `SUM(units) / COUNT(encounters)` | `billing_index.cpt_entries` | Target: 4–6 units |
| Average visit duration | `AVG(total_treatment_minutes)` | `pt_note_index.total_treatment_minutes` (from ProgressNoteFields) | 45–60 min |

### Financial KPIs

| KPI | Formula | Data Source | Normal Range |
|-----|---------|-------------|-------------|
| Revenue per visit | `SUM(amount_paid) / COUNT(encounters)` | `payment_posting_index` + `claim_index` | Solo PT: $80–$140 |
| Net collection rate | `SUM(payments) / (SUM(charges) - SUM(contractual_adj))` | `payment_posting_index` + `billing_index` | Target: > 95% |
| Days in A/R | `(SUM(outstanding_balance) / SUM(monthly_charges)) × 30` | `claim_index` + `payment_posting_index` | Target: < 30 days |
| Gross charges | `SUM(billing_index.total_charge)` | `billing_index` | — |
| Payments posted | `SUM(payment_posting_index.amount_paid)` | `payment_posting_index` | — |

### Clinical Outcomes KPIs

| KPI | Formula | Data Source |
|-----|---------|-------------|
| MCID achievement rate | `COUNT(episodes where change ≥ MCID) / COUNT(episodes)` per measure | `outcome_score_index` (M003/S02) |
| Improvement rate | `COUNT(patients with positive change) / COUNT(patients)` | `outcome_score_index` |
| Provider comparison | Per-provider outcome rates | `outcome_score_index` JOIN `pt_note_index.provider_id` |

### Payer Mix

| KPI | Formula | Data Source |
|-----|---------|-------------|
| Payer distribution | `COUNT(claims) per payer_id` | `claim_index` |
| Revenue by payer | `SUM(amount_paid) per payer_id` | `payment_posting_index` + `claim_index` |
| Denial rate by payer | `COUNT(denied) / COUNT(submitted) per payer_id` | `claim_index` |

## Aggregate SQL Patterns

### Visits Per Day (Date Range)
```sql
SELECT
  DATE(c.created_at) AS visit_date,
  COUNT(*) AS visit_count
FROM pt_note_index c
WHERE c.status IN ('signed', 'locked')
  AND c.created_at >= :start_date
  AND c.created_at <= :end_date
  AND c.note_type = 'progress_note'
GROUP BY DATE(c.created_at)
ORDER BY visit_date;
```

### Net Collection Rate
```sql
SELECT
  SUM(pp.amount_paid) AS total_payments,
  SUM(bi.total_charge) AS gross_charges,
  SUM(CASE WHEN pp.adjustment_type = 'contractual' THEN pp.adjustment ELSE 0 END) AS contractual_adj,
  SUM(pp.amount_paid) * 1.0 / NULLIF(
    SUM(bi.total_charge) - SUM(CASE WHEN pp.adjustment_type = 'contractual' THEN pp.adjustment ELSE 0 END), 0
  ) AS net_collection_rate
FROM payment_posting_index pp
JOIN claim_index ci ON pp.claim_id = ci.claim_id
JOIN billing_index bi ON ci.billing_id = bi.billing_id
WHERE bi.billed_date >= :start_date
  AND bi.billed_date <= :end_date;
```

### MCID Achievement Rate per Measure
```sql
SELECT
  osi_initial.measure_type,
  COUNT(*) AS total_episodes,
  COUNT(CASE
    WHEN ABS(osi_discharge.score - osi_initial.score) >= :mcid
    THEN 1 ELSE NULL END) AS mcid_achieved
FROM outcome_score_index osi_initial
JOIN outcome_score_index osi_discharge
  ON osi_initial.patient_id = osi_discharge.patient_id
  AND osi_initial.measure_type = osi_discharge.measure_type
WHERE osi_initial.episode_phase = 'initial'
  AND osi_discharge.episode_phase = 'discharge'
  AND osi_initial.recorded_at >= :start_date
GROUP BY osi_initial.measure_type;
```

### Payer Mix
```sql
SELECT
  ci.payer_id,
  pc.payer_name,
  COUNT(*) AS claim_count,
  SUM(pp.amount_paid) AS total_paid,
  COUNT(*) * 1.0 / (SELECT COUNT(*) FROM claim_index WHERE status NOT IN ('draft')) AS percentage
FROM claim_index ci
LEFT JOIN payment_posting_index pp ON pp.claim_id = ci.claim_id
LEFT JOIN payer_config pc ON ci.payer_id = pc.payer_id
WHERE ci.submitted_at >= :start_date
GROUP BY ci.payer_id;
```

## Dashboard Frameworks

### recharts Analysis for Tauri/WKWebView

recharts `2.12.x` is built on D3 + React 18. Key components needed:
- `LineChart` + `Line` — trend line for visits/day, outcome scores over time
- `BarChart` + `Bar` — revenue per visit, units per visit by month
- `PieChart` + `Pie` — payer mix distribution
- `AreaChart` — A/R aging waterfall
- `ResponsiveContainer` — auto-sizes chart to parent div

**Tauri WKWebView compatibility:**
- WKWebView uses WebKit rendering engine; recharts uses SVG (not Canvas); SVG rendering in WebKit is mature and correct
- No SSR or hydration issues in Tauri (no server)
- Dynamic imports not needed; standard `import { LineChart } from 'recharts'` works
- recharts and its peer dependency (`victory-vendor`) have no native module requirements — pure JS

**Bundle size:** recharts `2.12` adds ~250 KB minified + gzipped to the JS bundle. Acceptable for a desktop app.

### Alternative: Inline SVG (as in M003/S02)
For the 6 KPI widgets, inline SVG (without recharts) would work for simple bar charts. However, M004 explicitly plans recharts for the analytics and MIPS dashboards. Using inline SVG here would be inconsistent with the planned architecture and would require more custom code for the interactive features (tooltips, hover, legend).

**Decision: Use recharts for all M004 analytics charts.** The M003/S02 inline SVG trend chart remains for outcome score trends on individual patient records (a simpler, patient-level view).

## Widget Card Design

Each KPI widget is a standardised card:
```tsx
interface KpiWidgetProps {
  title: string;
  value: string | number;
  unit?: string;
  change?: number;       // % change from prior period
  chartData?: ChartDataPoint[];
  chartType: 'line' | 'bar' | 'pie' | 'number';
  loading: boolean;
  error?: string | null;
}
```

Widget layout:
- Title in small caps at top
- Large value with unit in centre
- Change arrow (▲ green, ▼ red, or — neutral) for period-over-period
- Mini sparkline chart below (recharts `LineChart` 150×50 without axes)
- Date range shows below chart

## Date Range Filter

A shared date range selector at the top of the dashboard drives all KPI queries. Preset options:
- Last 30 days
- Last 90 days
- Last 12 months
- Current calendar year
- Custom range (date picker)

The date range is passed as `{ start: string, end: string }` to each KPI command via `get_operational_kpis`, `get_financial_kpis`, etc.

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| recharts SSR safety in Tauri WKWebView | Low | Tauri has no SSR; standard import works; verify in T01 by mounting dashboard with live data |
| Aggregate SQL performance on large datasets | Low | Solo PT practice: < 10,000 encounters/year; indexed queries run in < 50 ms; no query optimisation needed |
| MCID values differ by measure type | Medium | Hard-code MCID per measure in `analytics.rs` constants (9 for LEFS, 10.8 for DASH, 7.5 for NDI, 10 for Oswestry, 2 for PSFS); pass as parameter to the SQL query |
| Missing data for some KPIs (e.g., no claims yet) | Low | All KPI commands return `null` for unavailable metrics; frontend shows "No data" state gracefully |
| recharts bundle size | Low | 250 KB gzipped for a desktop app is acceptable; no lazy loading needed |

## Sources

- recharts documentation: recharts.org / docs.recharts.org
- APTA PT practice benchmarks: apta.org/your-practice/business-and-practice-management
- CMS net collection rate benchmarks: mgma.com (MGMA Physician Compensation and Production Survey)
- Days in A/R benchmarks: hfma.org (Hospital & Healthcare Financial Management)
- MCID values: see M003/S02 research for per-measure MCID reference
- `appointment_index` schema: M001/M002 scheduling module (existing table)
