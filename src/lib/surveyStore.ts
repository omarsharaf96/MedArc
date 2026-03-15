/**
 * surveyStore.ts — LocalStorage-backed survey template & response store.
 *
 * Provides the same async interface as Tauri invoke() commands so that
 * the frontend pages work identically whether backed by localStorage
 * or real Rust commands. When the Rust survey commands land, replace
 * these functions with invoke() calls in tauri.ts.
 *
 * Storage keys:
 *   "medarc_survey_templates" → SurveyTemplate[]
 *   "medarc_survey_responses" → SurveyResponse[]
 */

import type {
  SurveyTemplate,
  SurveyTemplateInput,
  SurveyResponse,
  SurveyResponseInput,
} from "../types/survey";

// ─── Storage helpers ────────────────────────────────────────────────────────

const TEMPLATES_KEY = "medarc_survey_templates";
const RESPONSES_KEY = "medarc_survey_responses";

function generateId(): string {
  return crypto.randomUUID();
}

function readTemplates(): SurveyTemplate[] {
  try {
    const raw = localStorage.getItem(TEMPLATES_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as SurveyTemplate[];
  } catch {
    return [];
  }
}

function writeTemplates(templates: SurveyTemplate[]): void {
  localStorage.setItem(TEMPLATES_KEY, JSON.stringify(templates));
}

function readResponses(): SurveyResponse[] {
  try {
    const raw = localStorage.getItem(RESPONSES_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as SurveyResponse[];
  } catch {
    return [];
  }
}

function writeResponses(responses: SurveyResponse[]): void {
  localStorage.setItem(RESPONSES_KEY, JSON.stringify(responses));
}

// ─── Built-in templates (seeded on first access) ────────────────────────────

const BUILT_IN_TEMPLATES: Omit<SurveyTemplate, "id" | "createdAt" | "createdBy">[] = [
  {
    name: "Pain and Function Intake",
    builtIn: true,
    fields: [
      { id: "pf-1", label: "Current Pain Level", fieldType: "pain_scale", required: true, order: 0 },
      { id: "pf-2", label: "Pain Location", fieldType: "text", required: true, order: 1 },
      { id: "pf-3", label: "Pain Duration (days)", fieldType: "number", required: false, order: 2 },
      { id: "pf-4", label: "Pain started after an injury?", fieldType: "yes_no", required: true, order: 3 },
      { id: "pf-5", label: "Date pain began", fieldType: "date", required: false, order: 4 },
      { id: "pf-6", label: "Average Pain Level (past week)", fieldType: "pain_scale", required: true, order: 5 },
      { id: "pf-7", label: "Activities limited by pain", fieldType: "text", required: false, order: 6 },
    ],
  },
  {
    name: "Medical History",
    builtIn: true,
    fields: [
      { id: "mh-1", label: "Do you have any chronic conditions?", fieldType: "yes_no", required: true, order: 0 },
      { id: "mh-2", label: "List current medications", fieldType: "text", required: false, order: 1 },
      { id: "mh-3", label: "Any previous surgeries?", fieldType: "yes_no", required: true, order: 2 },
      { id: "mh-4", label: "Describe previous surgeries", fieldType: "text", required: false, order: 3 },
      { id: "mh-5", label: "Family history of heart disease?", fieldType: "yes_no", required: true, order: 4 },
      { id: "mh-6", label: "Family history of cancer?", fieldType: "yes_no", required: true, order: 5 },
      { id: "mh-7", label: "Additional medical history notes", fieldType: "text", required: false, order: 6 },
    ],
  },
  {
    name: "HIPAA Acknowledgment",
    builtIn: true,
    fields: [
      { id: "hp-1", label: "I acknowledge receiving the HIPAA Notice of Privacy Practices", fieldType: "yes_no", required: true, order: 0 },
      { id: "hp-2", label: "Date of acknowledgment", fieldType: "date", required: true, order: 1 },
      { id: "hp-3", label: "I consent to electronic communication", fieldType: "yes_no", required: true, order: 2 },
      { id: "hp-4", label: "Additional notes or concerns", fieldType: "text", required: false, order: 3 },
    ],
  },
];

function ensureBuiltIns(templates: SurveyTemplate[]): SurveyTemplate[] {
  const existingNames = new Set(templates.map((t) => t.name));
  let added = false;

  for (const builtIn of BUILT_IN_TEMPLATES) {
    if (!existingNames.has(builtIn.name)) {
      templates.push({
        ...builtIn,
        id: generateId(),
        createdAt: new Date().toISOString(),
        createdBy: "system",
      });
      added = true;
    }
  }

  if (added) {
    writeTemplates(templates);
  }

  return templates;
}

// ─── Public API (async to match Tauri invoke pattern) ───────────────────────

/** List all survey templates. Seeds built-in templates on first call. */
export async function listSurveyTemplates(): Promise<SurveyTemplate[]> {
  const templates = readTemplates();
  return ensureBuiltIns(templates);
}

/** Get a single survey template by ID. */
export async function getSurveyTemplate(templateId: string): Promise<SurveyTemplate | null> {
  const templates = await listSurveyTemplates();
  return templates.find((t) => t.id === templateId) ?? null;
}

/** Create a new survey template. Returns the created template. */
export async function createSurveyTemplate(input: SurveyTemplateInput): Promise<SurveyTemplate> {
  const templates = await listSurveyTemplates();

  const template: SurveyTemplate = {
    id: generateId(),
    name: input.name,
    fields: input.fields,
    builtIn: false,
    createdAt: new Date().toISOString(),
    createdBy: "current-user",
  };

  templates.push(template);
  writeTemplates(templates);
  return template;
}

/** Submit a survey response. Returns the saved response. */
export async function submitSurveyResponse(input: SurveyResponseInput): Promise<SurveyResponse> {
  const responses = readResponses();

  const response: SurveyResponse = {
    id: generateId(),
    templateId: input.templateId,
    patientId: input.patientId,
    responses: input.responses,
    submittedAt: new Date().toISOString(),
  };

  responses.push(response);
  writeResponses(responses);
  return response;
}
