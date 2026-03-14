/**
 * ObjectiveMeasuresPage.tsx — ROM/MMT/ortho-test recording and outcome score entry.
 *
 * Two tabs:
 *   - "Objective Measures" (ROM, MMT, ortho tests — placeholder for T02 body-region UI)
 *   - "Outcome Scores" (LEFS, DASH, NDI, Oswestry, PSFS, FABQ entry forms, trend chart)
 *
 * The Outcome Scores tab supports all six standardised measures with per-item input,
 * auto-displayed score + severity from the backend, a score history table, and an
 * inline SVG trend chart (ScoreChart).
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import { useNav } from "../contexts/RouterContext";
import type {
  MeasureType,
  EpisodePhase,
  OutcomeScoreRecord,
} from "../types/pt";

// ─── Props ───────────────────────────────────────────────────────────────────

interface Props {
  patientId: string;
  role: string;
  userId: string;
}

// ─── Tailwind constants ──────────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:bg-gray-50 disabled:text-gray-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── MCID and max score constants ────────────────────────────────────────────

const MEASURE_MCID: Record<MeasureType, number> = {
  lefs: 9,
  dash: 10.8,
  ndi: 7.5,
  oswestry: 10,
  psfs: 2,
  fabq: 5,
};

const MEASURE_MAX_SCORE: Record<MeasureType, number> = {
  lefs: 80,
  dash: 100,
  ndi: 100,
  oswestry: 100,
  psfs: 10,
  fabq: 42,
};

const MEASURE_LABELS: Record<MeasureType, string> = {
  lefs: "LEFS",
  dash: "DASH",
  ndi: "NDI",
  oswestry: "Oswestry",
  psfs: "PSFS",
  fabq: "FABQ",
};

const ALL_MEASURES: MeasureType[] = ["lefs", "dash", "ndi", "oswestry", "psfs", "fabq"];

// ─── LEFS items (20 items, 0-4) ─────────────────────────────────────────────

const LEFS_ITEMS: string[] = [
  "Any of your usual work, housework, or school activities",
  "Your usual hobbies, recreational, or sporting activities",
  "Getting into or out of the bath",
  "Walking between rooms",
  "Putting on your shoes or socks",
  "Squatting",
  "Lifting an object, like a bag of groceries, from the floor",
  "Performing light activities around your home",
  "Performing heavy activities around your home",
  "Getting into or out of a car",
  "Walking 2 blocks",
  "Walking a mile",
  "Going up or down 10 stairs (about 1 flight of stairs)",
  "Standing for 1 hour",
  "Sitting for 1 hour",
  "Running on even ground",
  "Running on uneven ground",
  "Making sharp turns while running fast",
  "Hopping",
  "Rolling over in bed",
];

// ─── DASH items (30 items, 1-5) ─────────────────────────────────────────────

const DASH_ITEMS: string[] = [
  "Open a tight or new jar",
  "Write",
  "Turn a key",
  "Prepare a meal",
  "Push open a heavy door",
  "Place an object on a shelf above your head",
  "Do heavy household chores (e.g., wash walls, floors)",
  "Garden or do yard work",
  "Make a bed",
  "Carry a shopping bag or briefcase",
  "Carry a heavy object (over 10 lbs)",
  "Change a lightbulb overhead",
  "Wash or blow dry your hair",
  "Wash your back",
  "Put on a pullover sweater",
  "Use a knife to cut food",
  "Recreational activities requiring little effort (e.g., card playing)",
  "Recreational activities with some force through arm/shoulder/hand (e.g., golf, hammering)",
  "Recreational activities that move arm freely (e.g., throwing, badminton)",
  "Manage transportation needs (get from one place to another)",
  "Sexual activities",
  "Severity of arm, shoulder, or hand pain",
  "Severity of arm, shoulder, or hand pain during specific activity",
  "Tingling (pins and needles) in your arm, shoulder, or hand",
  "Weakness in your arm, shoulder, or hand",
  "Stiffness in your arm, shoulder, or hand",
  "Difficulty sleeping due to pain in arm, shoulder, or hand",
  "I feel less capable, less confident, or less useful because of my arm, shoulder, or hand problem",
  "Interference of arm, shoulder, or hand problem with normal social activities",
  "Limitation in work or other regular daily activities as a result of arm, shoulder, or hand problem",
];

// ─── NDI items (10 items, 0-5) ──────────────────────────────────────────────

const NDI_ITEMS: { label: string; options: string[] }[] = [
  {
    label: "Pain Intensity",
    options: [
      "0 - I have no pain at the moment",
      "1 - The pain is very mild at the moment",
      "2 - The pain is moderate at the moment",
      "3 - The pain is fairly severe at the moment",
      "4 - The pain is very severe at the moment",
      "5 - The pain is the worst imaginable at the moment",
    ],
  },
  {
    label: "Personal Care (Washing, Dressing, etc.)",
    options: [
      "0 - I can look after myself normally without causing extra pain",
      "1 - I can look after myself normally but it causes extra pain",
      "2 - It is painful to look after myself and I am slow and careful",
      "3 - I need some help but manage most of my personal care",
      "4 - I need help every day in most aspects of self-care",
      "5 - I do not get dressed, I wash with difficulty and stay in bed",
    ],
  },
  {
    label: "Lifting",
    options: [
      "0 - I can lift heavy weights without extra pain",
      "1 - I can lift heavy weights but it gives me extra pain",
      "2 - Pain prevents me from lifting heavy weights off the floor, but I can manage if they are conveniently positioned",
      "3 - Pain prevents me from lifting heavy weights, but I can manage light to medium weights if they are conveniently positioned",
      "4 - I can only lift very light weights",
      "5 - I cannot lift or carry anything at all",
    ],
  },
  {
    label: "Reading",
    options: [
      "0 - I can read as much as I want to with no pain in my neck",
      "1 - I can read as much as I want to with slight pain in my neck",
      "2 - I can read as much as I want with moderate pain in my neck",
      "3 - I can't read as much as I want because of moderate pain in my neck",
      "4 - I can hardly read at all because of severe pain in my neck",
      "5 - I cannot read at all",
    ],
  },
  {
    label: "Headaches",
    options: [
      "0 - I have no headaches at all",
      "1 - I have slight headaches which come infrequently",
      "2 - I have moderate headaches which come infrequently",
      "3 - I have moderate headaches which come frequently",
      "4 - I have severe headaches which come frequently",
      "5 - I have headaches almost all the time",
    ],
  },
  {
    label: "Concentration",
    options: [
      "0 - I can concentrate fully when I want to with no difficulty",
      "1 - I can concentrate fully when I want to with slight difficulty",
      "2 - I have a fair degree of difficulty in concentrating when I want to",
      "3 - I have a lot of difficulty in concentrating when I want to",
      "4 - I have a great deal of difficulty in concentrating when I want to",
      "5 - I cannot concentrate at all",
    ],
  },
  {
    label: "Work",
    options: [
      "0 - I can do as much work as I want to",
      "1 - I can only do my usual work, but no more",
      "2 - I can do most of my usual work, but no more",
      "3 - I cannot do my usual work",
      "4 - I can hardly do any work at all",
      "5 - I can't do any work at all",
    ],
  },
  {
    label: "Driving",
    options: [
      "0 - I can drive my car without any neck pain",
      "1 - I can drive my car as long as I want with slight pain in my neck",
      "2 - I can drive my car as long as I want with moderate pain in my neck",
      "3 - I can't drive my car as long as I want because of moderate pain in my neck",
      "4 - I can hardly drive at all because of severe pain in my neck",
      "5 - I can't drive my car at all",
    ],
  },
  {
    label: "Sleeping",
    options: [
      "0 - I have no trouble sleeping",
      "1 - My sleep is slightly disturbed (less than 1 hr sleepless)",
      "2 - My sleep is mildly disturbed (1-2 hrs sleepless)",
      "3 - My sleep is moderately disturbed (2-3 hrs sleepless)",
      "4 - My sleep is greatly disturbed (3-5 hrs sleepless)",
      "5 - My sleep is completely disturbed (5-7 hrs sleepless)",
    ],
  },
  {
    label: "Recreation",
    options: [
      "0 - I am able to engage in all my recreation activities with no neck pain at all",
      "1 - I am able to engage in all my recreation activities, with some pain in my neck",
      "2 - I am able to engage in most, but not all of my usual recreation activities because of pain in my neck",
      "3 - I am able to engage in a few of my usual recreation activities because of pain in my neck",
      "4 - I can hardly do any recreation activities because of pain in my neck",
      "5 - I can't do any recreation activities at all",
    ],
  },
];

// ─── Oswestry items (10 items, 0-5) ─────────────────────────────────────────

const OSWESTRY_ITEMS: { label: string; options: string[] }[] = [
  {
    label: "Pain Intensity",
    options: [
      "0 - I have no pain at the moment",
      "1 - The pain is very mild at the moment",
      "2 - The pain is moderate at the moment",
      "3 - The pain is fairly severe at the moment",
      "4 - The pain is very severe at the moment",
      "5 - The pain is the worst imaginable at the moment",
    ],
  },
  {
    label: "Personal Care",
    options: [
      "0 - I can look after myself normally without causing extra pain",
      "1 - I can look after myself normally but it causes extra pain",
      "2 - It is painful to look after myself and I am slow and careful",
      "3 - I need some help but manage most of my personal care",
      "4 - I need help every day in most aspects of self-care",
      "5 - I do not get dressed, wash with difficulty and stay in bed",
    ],
  },
  {
    label: "Lifting",
    options: [
      "0 - I can lift heavy weights without extra pain",
      "1 - I can lift heavy weights but it gives extra pain",
      "2 - Pain prevents me from lifting heavy weights off the floor",
      "3 - Pain prevents me from lifting heavy weights but I can manage light to medium weights",
      "4 - I can only lift very light weights",
      "5 - I cannot lift or carry anything at all",
    ],
  },
  {
    label: "Walking",
    options: [
      "0 - Pain does not prevent me walking any distance",
      "1 - Pain prevents me from walking more than 1 mile",
      "2 - Pain prevents me from walking more than 1/2 mile",
      "3 - Pain prevents me from walking more than 100 yards",
      "4 - I can only walk using a stick or crutches",
      "5 - I am in bed most of the time",
    ],
  },
  {
    label: "Sitting",
    options: [
      "0 - I can sit in any chair as long as I like",
      "1 - I can only sit in my favourite chair as long as I like",
      "2 - Pain prevents me sitting more than one hour",
      "3 - Pain prevents me from sitting more than 30 minutes",
      "4 - Pain prevents me from sitting more than 10 minutes",
      "5 - Pain prevents me from sitting at all",
    ],
  },
  {
    label: "Standing",
    options: [
      "0 - I can stand as long as I want without extra pain",
      "1 - I can stand as long as I want but it gives me extra pain",
      "2 - Pain prevents me from standing for more than 1 hour",
      "3 - Pain prevents me from standing for more than 30 minutes",
      "4 - Pain prevents me from standing for more than 10 minutes",
      "5 - Pain prevents me from standing at all",
    ],
  },
  {
    label: "Sleeping",
    options: [
      "0 - My sleep is never disturbed by pain",
      "1 - My sleep is occasionally disturbed by pain",
      "2 - Because of pain I have less than 6 hours sleep",
      "3 - Because of pain I have less than 4 hours sleep",
      "4 - Because of pain I have less than 2 hours sleep",
      "5 - Pain prevents me from sleeping at all",
    ],
  },
  {
    label: "Sex Life (if applicable)",
    options: [
      "0 - My sex life is normal and causes no extra pain",
      "1 - My sex life is normal but causes some extra pain",
      "2 - My sex life is nearly normal but is very painful",
      "3 - My sex life is severely restricted by pain",
      "4 - My sex life is nearly absent because of pain",
      "5 - Pain prevents any sex life at all",
    ],
  },
  {
    label: "Social Life",
    options: [
      "0 - My social life is normal and gives me no extra pain",
      "1 - My social life is normal but increases the degree of pain",
      "2 - Pain has no significant effect on my social life apart from limiting my more energetic interests",
      "3 - Pain has restricted my social life and I do not go out as often",
      "4 - Pain has restricted my social life to my home",
      "5 - I have no social life because of pain",
    ],
  },
  {
    label: "Travelling",
    options: [
      "0 - I can travel anywhere without pain",
      "1 - I can travel anywhere but it gives me extra pain",
      "2 - Pain is bad but I manage journeys over two hours",
      "3 - Pain restricts me to journeys of less than one hour",
      "4 - Pain restricts me to short necessary journeys under 30 minutes",
      "5 - Pain prevents me from travelling except to receive treatment",
    ],
  },
];

// ─── FABQ items (16 items, 0-6) ─────────────────────────────────────────────

const FABQ_ITEMS: string[] = [
  "My pain was caused by physical activity",
  "Physical activity makes my pain worse",
  "Physical activity might harm my back",
  "I should not do physical activities which (might) make my pain worse",
  "I cannot do physical activities which (might) make my pain worse",
  "My pain was caused by my work or by an accident at work",
  "My work aggravated my pain",
  "I have a claim for compensation for my pain",
  "My work is too heavy for me",
  "My work makes or would make my pain worse",
  "My work might harm my back",
  "I should not do my normal work with my present pain",
  "I cannot do my normal work with my present pain",
  "I cannot do my normal work till my pain is treated",
  "I do not think that I will be back to my normal work within 3 months",
  "I do not think that I will ever be able to go back to that work",
];

/** FABQ items that are NOT scored (1-indexed: 1, 8, 16 => 0-indexed: 0, 7, 15). */
const FABQ_NOT_SCORED: Set<number> = new Set([0, 7, 15]);

// ─── ScoreChart component ────────────────────────────────────────────────────

interface ScoreChartProps {
  scores: { date: string; score: number }[];
  maxScore: number;
  label: string;
}

function ScoreChart({ scores, maxScore, label }: ScoreChartProps) {
  const width = 400;
  const padX = 20;
  const plotLeft = padX;
  const plotRight = width - padX;
  const plotWidth = plotRight - plotLeft;
  const yTop = 10;
  const yBottom = 110;
  const plotHeight = yBottom - yTop;

  function computeY(score: number): number {
    const ratio = Math.max(0, Math.min(1, score / maxScore));
    return yBottom - ratio * plotHeight;
  }

  function computeX(index: number, total: number): number {
    if (total <= 1) return plotLeft + plotWidth / 2;
    return plotLeft + (index / (total - 1)) * plotWidth;
  }

  return (
    <svg viewBox={`0 0 ${width} 120`} className="w-full max-w-lg" role="img" aria-label={`${label} trend chart`}>
      {/* Title */}
      <text x={width / 2} y={8} textAnchor="middle" className="text-xs" fill="#6b7280" fontSize="10">
        {label}
      </text>

      {/* Y-axis reference lines */}
      <line x1={plotLeft} y1={yTop} x2={plotRight} y2={yTop} stroke="#e5e7eb" strokeWidth="1" />
      <line x1={plotLeft} y1={yBottom} x2={plotRight} y2={yBottom} stroke="#e5e7eb" strokeWidth="1" />
      <text x={plotLeft - 2} y={yTop + 4} textAnchor="end" fill="#9ca3af" fontSize="8">
        {maxScore}
      </text>
      <text x={plotLeft - 2} y={yBottom + 3} textAnchor="end" fill="#9ca3af" fontSize="8">
        0
      </text>

      {scores.length === 0 && (
        <text x={width / 2} y={65} textAnchor="middle" fill="#9ca3af" fontSize="11">
          No data
        </text>
      )}

      {scores.length === 1 && (() => {
        const x = computeX(0, 1);
        const y = computeY(scores[0].score);
        return (
          <>
            <circle cx={x} cy={y} r={4} fill="#6366f1" />
            <text x={x} y={y + 14} textAnchor="middle" fill="#4f46e5" fontSize="9">
              {scores[0].score}
            </text>
            <text x={x} y={yBottom + 10} textAnchor="middle" fill="#9ca3af" fontSize="7">
              {scores[0].date.slice(0, 10)}
            </text>
          </>
        );
      })()}

      {scores.length >= 2 && (() => {
        const points = scores.map((s, i) => ({
          x: computeX(i, scores.length),
          y: computeY(s.score),
          score: s.score,
          date: s.date,
        }));
        const pointsStr = points.map((p) => `${p.x},${p.y}`).join(" ");
        return (
          <>
            <polyline
              points={pointsStr}
              fill="none"
              stroke="#6366f1"
              strokeWidth="2"
              strokeLinejoin="round"
            />
            {points.map((p, i) => (
              <g key={i}>
                <circle cx={p.x} cy={p.y} r={3} fill="#6366f1" />
                <text x={p.x} y={p.y - 6} textAnchor="middle" fill="#4f46e5" fontSize="8">
                  {p.score}
                </text>
              </g>
            ))}
          </>
        );
      })()}
    </svg>
  );
}

// ─── PSFS Activity Row ───────────────────────────────────────────────────────

interface PsfsActivity {
  name: string;
  score: string;
}

// ─── Measure Entry Forms ─────────────────────────────────────────────────────

/** Generic numbered item input for simple 0-N scored measures (LEFS, DASH). */
function NumberedItemInput({
  index,
  label,
  value,
  min,
  max,
  onChange,
  inputId,
}: {
  index: number;
  label: string;
  value: string;
  min: number;
  max: number;
  onChange: (val: string) => void;
  inputId?: string;
}) {
  const id = inputId ?? `numbered-item-${index}`;
  return (
    <div className="flex items-start gap-3 py-1.5 border-b border-gray-50 last:border-0">
      <span className="mt-2 w-6 shrink-0 text-right text-xs font-medium text-gray-400">{index + 1}.</span>
      <div className="flex-1">
        <label htmlFor={id} className="block text-xs text-gray-600 mb-0.5">{label}</label>
        <input
          id={id}
          type="number"
          min={min}
          max={max}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={INPUT_CLS + " max-w-[80px]"}
          placeholder={`${min}-${max}`}
        />
      </div>
    </div>
  );
}

/** Select-based item for NDI / Oswestry. */
function SelectItemInput({
  index,
  label,
  options,
  value,
  onChange,
  inputId,
}: {
  index: number;
  label: string;
  options: string[];
  value: string;
  onChange: (val: string) => void;
  inputId?: string;
}) {
  const id = inputId ?? `select-item-${index}`;
  return (
    <div className="py-2 border-b border-gray-50 last:border-0">
      <div className="flex items-center gap-2 mb-1">
        <span className="w-6 shrink-0 text-right text-xs font-medium text-gray-400">{index + 1}.</span>
        <label htmlFor={id} className={LABEL_CLS + " mb-0"}>{label}</label>
      </div>
      <div className="ml-8">
        <select
          id={id}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={INPUT_CLS}
        >
          <option value="">-- Select --</option>
          {options.map((opt, i) => (
            <option key={i} value={String(i)}>
              {opt}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}

// ─── Main Component ──────────────────────────────────────────────────────────

export function ObjectiveMeasuresPage({ patientId, role: _role, userId: _userId }: Props) {
  const { goBack } = useNav();
  const [activeTab, setActiveTab] = useState<"objective" | "scores">("scores");

  // ── Outcome Scores tab state ───────────────────────────────────────────
  const [selectedMeasure, setSelectedMeasure] = useState<MeasureType | null>(null);
  const [episodePhase, setEpisodePhase] = useState<EpisodePhase>("mid");

  // Per-measure item values (stored as string to allow empty)
  const [lefsItems, setLefsItems] = useState<string[]>(Array(20).fill(""));
  const [dashItems, setDashItems] = useState<string[]>(Array(30).fill(""));
  const [ndiItems, setNdiItems] = useState<string[]>(Array(10).fill(""));
  const [oswestryItems, setOswestryItems] = useState<string[]>(Array(10).fill(""));
  const [fabqItems, setFabqItems] = useState<string[]>(Array(16).fill(""));
  const [psfsActivities, setPsfsActivities] = useState<PsfsActivity[]>([
    { name: "", score: "" },
    { name: "", score: "" },
    { name: "", score: "" },
  ]);

  // Score & Save state
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [lastResult, setLastResult] = useState<OutcomeScoreRecord | null>(null);

  // Score history
  const [history, setHistory] = useState<OutcomeScoreRecord[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);

  // ── Load score history for selected measure ────────────────────────────
  const loadHistory = useCallback(
    (measure: MeasureType | null) => {
      setHistoryLoading(true);
      commands
        .listOutcomeScores(patientId, measure)
        .then((records) => setHistory(records))
        .catch(() => setHistory([]))
        .finally(() => setHistoryLoading(false));
    },
    [patientId],
  );

  useEffect(() => {
    if (selectedMeasure) {
      loadHistory(selectedMeasure);
    } else {
      setHistory([]);
    }
  }, [selectedMeasure, loadHistory]);

  // ── Collect items for selected measure ─────────────────────────────────
  function collectItems(): number[] {
    switch (selectedMeasure) {
      case "lefs":
        return lefsItems.map((v) => {
          const n = parseInt(v, 10);
          return isNaN(n) ? 0 : n;
        });
      case "dash":
        return dashItems.map((v) => {
          const n = parseInt(v, 10);
          return isNaN(n) ? 1 : n;
        });
      case "ndi":
        return ndiItems.map((v) => {
          const n = parseInt(v, 10);
          return isNaN(n) ? 0 : n;
        });
      case "oswestry":
        return oswestryItems.map((v) => {
          const n = parseInt(v, 10);
          return isNaN(n) ? 0 : n;
        });
      case "psfs":
        return psfsActivities.map((a) => {
          const n = parseInt(a.score, 10);
          return isNaN(n) ? 0 : n;
        });
      case "fabq":
        return fabqItems.map((v) => {
          const n = parseInt(v, 10);
          return isNaN(n) ? 0 : n;
        });
      default:
        return [];
    }
  }

  // ── Score & Save handler ───────────────────────────────────────────────
  async function handleScoreAndSave() {
    if (!selectedMeasure) return;
    setSaving(true);
    setSaveError(null);
    setLastResult(null);

    const items = collectItems();

    try {
      const result = await commands.recordOutcomeScore({
        patientId,
        encounterId: null,
        measureType: selectedMeasure,
        items,
        episodePhase,
      });
      setLastResult(result);
      loadHistory(selectedMeasure);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error(`[ObjectiveMeasuresPage] recordOutcomeScore failed:`, msg);
      setSaveError(msg);
    } finally {
      setSaving(false);
    }
  }

  // ── Reset items when measure changes ───────────────────────────────────
  function handleMeasureSelect(m: MeasureType) {
    setSelectedMeasure(m);
    setLastResult(null);
    setSaveError(null);
    // Reset item values
    setLefsItems(Array(20).fill(""));
    setDashItems(Array(30).fill(""));
    setNdiItems(Array(10).fill(""));
    setOswestryItems(Array(10).fill(""));
    setFabqItems(Array(16).fill(""));
    setPsfsActivities([
      { name: "", score: "" },
      { name: "", score: "" },
      { name: "", score: "" },
    ]);
  }

  // ── Helper: update PSFS activity ───────────────────────────────────────
  function updatePsfsActivity(index: number, field: "name" | "score", value: string) {
    setPsfsActivities((prev) => {
      const next = [...prev];
      next[index] = { ...next[index], [field]: value };
      return next;
    });
  }

  function addPsfsActivity() {
    if (psfsActivities.length < 5) {
      setPsfsActivities((prev) => [...prev, { name: "", score: "" }]);
    }
  }

  // ── Helper: update array item ──────────────────────────────────────────
  function updateItem(
    setter: React.Dispatch<React.SetStateAction<string[]>>,
    index: number,
    value: string,
  ) {
    setter((prev) => {
      const next = [...prev];
      next[index] = value;
      return next;
    });
  }

  // ── Prepare chart data ─────────────────────────────────────────────────
  const chartData = history
    .slice()
    .sort((a, b) => a.recordedAt.localeCompare(b.recordedAt))
    .map((r) => ({ date: r.recordedAt, score: r.score }));

  // ── Render ─────────────────────────────────────────────────────────────
  return (
    <div className="space-y-6 p-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={goBack}
          className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
          aria-label="Go back"
        >
          &larr; Back
        </button>
        <h1 className="text-xl font-bold text-gray-900">Objective Measures</h1>
      </div>

      {/* Tab bar */}
      <div className="flex gap-0 border-b border-gray-200" role="tablist" aria-label="Objective measures sections">
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "objective"}
          onClick={() => setActiveTab("objective")}
          className={[
            "px-4 py-2 text-sm font-medium border-b-2 transition-colors",
            activeTab === "objective"
              ? "border-indigo-500 text-indigo-600"
              : "border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300",
          ].join(" ")}
        >
          Objective Measures
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "scores"}
          onClick={() => setActiveTab("scores")}
          className={[
            "px-4 py-2 text-sm font-medium border-b-2 transition-colors",
            activeTab === "scores"
              ? "border-indigo-500 text-indigo-600"
              : "border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300",
          ].join(" ")}
        >
          Outcome Scores
        </button>
      </div>

      {/* ─── Objective Measures tab (placeholder — built in T02) ─────── */}
      {activeTab === "objective" && (
        <div className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
          <p className="text-sm text-gray-500">
            ROM / MMT / ortho-test recording UI will be implemented in a future task.
          </p>
        </div>
      )}

      {/* ─── Outcome Scores tab ──────────────────────────────────────── */}
      {activeTab === "scores" && (
        <div className="space-y-6">
          {/* Measure type selector — segmented control */}
          <div className="flex flex-wrap gap-2">
            {ALL_MEASURES.map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => handleMeasureSelect(m)}
                aria-label={`Select ${MEASURE_LABELS[m]} measure`}
                aria-pressed={selectedMeasure === m}
                className={[
                  "rounded-md px-4 py-2 text-sm font-medium transition-colors",
                  selectedMeasure === m
                    ? "bg-indigo-600 text-white"
                    : "bg-gray-100 text-gray-700 hover:bg-gray-200",
                ].join(" ")}
              >
                {MEASURE_LABELS[m]}
              </button>
            ))}
          </div>

          {selectedMeasure === null && (
            <div className="rounded-lg border border-gray-200 bg-white p-6 text-sm text-gray-500">
              Select a measure above to begin entry.
            </div>
          )}

          {selectedMeasure !== null && (
            <>
              {/* Episode phase radio group */}
              <div className="flex items-center gap-4">
                <span className="text-sm font-medium text-gray-700">Episode Phase:</span>
                {(["initial", "mid", "discharge"] as EpisodePhase[]).map((phase) => (
                  <label key={phase} className="flex items-center gap-1.5 text-sm text-gray-600">
                    <input
                      type="radio"
                      name="episodePhase"
                      value={phase}
                      checked={episodePhase === phase}
                      onChange={() => setEpisodePhase(phase)}
                      className="h-4 w-4 text-indigo-600 focus:ring-indigo-500"
                    />
                    {phase.charAt(0).toUpperCase() + phase.slice(1)}
                  </label>
                ))}
              </div>

              {/* Per-measure entry form */}
              <div className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
                <h3 className="mb-4 text-base font-semibold text-gray-800">
                  {MEASURE_LABELS[selectedMeasure]} Items
                </h3>

                {/* LEFS: 20 items, 0-4 */}
                {selectedMeasure === "lefs" && (
                  <div className="space-y-0">
                    {LEFS_ITEMS.map((label, i) => (
                      <NumberedItemInput
                        key={i}
                        index={i}
                        label={label}
                        value={lefsItems[i]}
                        min={0}
                        max={4}
                        onChange={(val) => updateItem(setLefsItems, i, val)}
                        inputId={`lefs-item-${i}`}
                      />
                    ))}
                  </div>
                )}

                {/* DASH: 30 items, 1-5 */}
                {selectedMeasure === "dash" && (
                  <div className="space-y-0">
                    <p className="mb-3 rounded-md bg-blue-50 px-3 py-2 text-xs text-blue-700">
                      27 or more items must be answered for a valid DASH score.
                    </p>
                    {DASH_ITEMS.map((label, i) => (
                      <NumberedItemInput
                        key={i}
                        index={i}
                        label={label}
                        value={dashItems[i]}
                        min={1}
                        max={5}
                        onChange={(val) => updateItem(setDashItems, i, val)}
                        inputId={`dash-item-${i}`}
                      />
                    ))}
                  </div>
                )}

                {/* NDI: 10 items, 0-5 */}
                {selectedMeasure === "ndi" && (
                  <div className="space-y-0">
                    {NDI_ITEMS.map((item, i) => (
                      <SelectItemInput
                        key={i}
                        index={i}
                        label={item.label}
                        options={item.options}
                        value={ndiItems[i]}
                        onChange={(val) => updateItem(setNdiItems, i, val)}
                        inputId={`ndi-item-${i}`}
                      />
                    ))}
                  </div>
                )}

                {/* Oswestry: 10 items, 0-5 */}
                {selectedMeasure === "oswestry" && (
                  <div className="space-y-0">
                    {OSWESTRY_ITEMS.map((item, i) => (
                      <SelectItemInput
                        key={i}
                        index={i}
                        label={item.label}
                        options={item.options}
                        value={oswestryItems[i]}
                        onChange={(val) => updateItem(setOswestryItems, i, val)}
                        inputId={`oswestry-item-${i}`}
                      />
                    ))}
                  </div>
                )}

                {/* PSFS: 3-5 activities, each 0-10 */}
                {selectedMeasure === "psfs" && (
                  <div className="space-y-3">
                    {psfsActivities.map((activity, i) => (
                      <div key={i} className="flex items-end gap-3 border-b border-gray-50 pb-3 last:border-0">
                        <span className="mt-2 w-6 shrink-0 text-right text-xs font-medium text-gray-400">{i + 1}.</span>
                        <div className="flex-1">
                          <label htmlFor={`psfs-name-${i}`} className="block text-xs text-gray-600 mb-0.5">Activity name</label>
                          <input
                            id={`psfs-name-${i}`}
                            type="text"
                            value={activity.name}
                            onChange={(e) => updatePsfsActivity(i, "name", e.target.value)}
                            className={INPUT_CLS}
                            placeholder="e.g., Walking, Climbing stairs"
                          />
                        </div>
                        <div className="w-24 shrink-0">
                          <label htmlFor={`psfs-score-${i}`} className="block text-xs text-gray-600 mb-0.5">Score (0-10)</label>
                          <input
                            id={`psfs-score-${i}`}
                            type="number"
                            min={0}
                            max={10}
                            value={activity.score}
                            onChange={(e) => updatePsfsActivity(i, "score", e.target.value)}
                            className={INPUT_CLS}
                            placeholder="0-10"
                          />
                        </div>
                      </div>
                    ))}
                    {psfsActivities.length < 5 && (
                      <button
                        type="button"
                        onClick={addPsfsActivity}
                        className="text-sm text-indigo-600 hover:text-indigo-800"
                      >
                        + Add activity
                      </button>
                    )}
                  </div>
                )}

                {/* FABQ: 16 items, 0-6 */}
                {selectedMeasure === "fabq" && (
                  <div className="space-y-0">
                    {FABQ_ITEMS.map((label, i) => {
                      const notScored = FABQ_NOT_SCORED.has(i);
                      const inputId = `fabq-item-${i}`;
                      return (
                        <div
                          key={i}
                          className={[
                            "flex items-start gap-3 py-1.5 border-b border-gray-50 last:border-0",
                            notScored ? "opacity-60" : "",
                          ].join(" ")}
                        >
                          <span className="mt-2 w-6 shrink-0 text-right text-xs font-medium text-gray-400">
                            {i + 1}.
                          </span>
                          <div className="flex-1">
                            <label htmlFor={inputId} className="block text-xs text-gray-600 mb-0.5">
                              {label}
                              {notScored && (
                                <span className="ml-1 text-gray-400">(not scored)</span>
                              )}
                            </label>
                            <input
                              id={inputId}
                              type="number"
                              min={0}
                              max={6}
                              value={fabqItems[i]}
                              onChange={(e) => updateItem(setFabqItems, i, e.target.value)}
                              className={INPUT_CLS + " max-w-[80px]"}
                              placeholder="0-6"
                            />
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>

              {/* Score & Save button */}
              <div className="flex items-center gap-4">
                <button
                  type="button"
                  onClick={handleScoreAndSave}
                  disabled={saving}
                  className="rounded-md bg-indigo-600 px-6 py-2.5 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-60"
                >
                  {saving ? "Scoring..." : "Score & Save"}
                </button>
              </div>

              {/* Save error */}
              {saveError && (
                <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  <p className="font-semibold">Score & Save failed</p>
                  <p className="mt-0.5">{saveError}</p>
                </div>
              )}

              {/* Result card */}
              {lastResult && (
                <div className="rounded-lg border border-green-200 bg-green-50 p-4">
                  <p className="text-sm font-semibold text-green-800">Score Recorded</p>
                  <div className="mt-2 space-y-1 text-sm text-green-700">
                    {selectedMeasure === "fabq" ? (
                      <>
                        <p>
                          <span className="font-medium">Work Subscale Score:</span>{" "}
                          {lastResult.score}
                        </p>
                        <p>
                          <span className="font-medium">PA Subscale Score:</span>{" "}
                          {lastResult.scoreSecondary ?? "N/A"}
                        </p>
                      </>
                    ) : (
                      <p>
                        <span className="font-medium">Score:</span> {lastResult.score}
                      </p>
                    )}
                    <p>
                      <span className="font-medium">Severity:</span>{" "}
                      <span className="inline-flex rounded-full bg-green-100 px-2 py-0.5 text-xs font-medium text-green-800 capitalize">
                        {(lastResult.severity ?? "unknown").replace(/_/g, " ")}
                      </span>
                    </p>
                    <p className="text-xs text-green-600">
                      MCID threshold: {MEASURE_MCID[selectedMeasure]} points
                    </p>
                  </div>
                </div>
              )}

              {/* Trend chart */}
              <div className="rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
                <ScoreChart
                  scores={chartData}
                  maxScore={MEASURE_MAX_SCORE[selectedMeasure]}
                  label={`${MEASURE_LABELS[selectedMeasure]} Trend`}
                />
              </div>

              {/* Score history table */}
              <div className="rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
                <h3 className="mb-3 text-sm font-semibold text-gray-800">Score History</h3>
                {historyLoading ? (
                  <p className="text-sm text-gray-500">Loading history...</p>
                ) : history.length === 0 ? (
                  <p className="text-sm text-gray-500">No scores recorded yet for this measure.</p>
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full text-sm">
                      <thead>
                        <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                          <th className="pb-2 pr-4">Date</th>
                          <th className="pb-2 pr-4">Phase</th>
                          <th className="pb-2 pr-4">Score</th>
                          {selectedMeasure === "fabq" && <th className="pb-2 pr-4">PA Score</th>}
                          <th className="pb-2">Severity</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-gray-50">
                        {history.map((record) => (
                          <tr key={record.scoreId}>
                            <td className="py-2 pr-4 text-gray-700">
                              {record.recordedAt.slice(0, 10)}
                            </td>
                            <td className="py-2 pr-4 capitalize text-gray-700">
                              {record.episodePhase}
                            </td>
                            <td className="py-2 pr-4 text-gray-900 font-medium">
                              {record.score}
                            </td>
                            {selectedMeasure === "fabq" && (
                              <td className="py-2 pr-4 text-gray-900 font-medium">
                                {record.scoreSecondary ?? "N/A"}
                              </td>
                            )}
                            <td className="py-2">
                              <span className="inline-flex rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium text-gray-700 capitalize">
                                {(record.severity ?? "unknown").replace(/_/g, " ")}
                              </span>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
