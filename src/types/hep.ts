/**
 * hep.ts — TypeScript types for the Home Exercise Program (HEP) Builder.
 *
 * Mirrors the Rust structs in src-tauri/src/commands/hep.rs.
 */

// ─── Enums ───────────────────────────────────────────────────────────────────

export type ExerciseRegion =
  | "cervical"
  | "thoracic"
  | "lumbar"
  | "shoulder"
  | "elbow"
  | "wrist"
  | "hip"
  | "knee"
  | "ankle"
  | "general";

export type ExerciseCategory =
  | "rom"
  | "strengthening"
  | "stretching"
  | "balance"
  | "functional"
  | "cardio";

export type ExerciseDifficulty = "beginner" | "intermediate" | "advanced";

// ─── Exercise library ────────────────────────────────────────────────────────

export interface Exercise {
  exerciseId: string;
  name: string;
  bodyRegion: ExerciseRegion;
  category: ExerciseCategory;
  description: string | null;
  instructions: string | null;
  equipment: string | null;
  difficulty: ExerciseDifficulty | null;
  isBuiltin: boolean;
}

// ─── Prescription ────────────────────────────────────────────────────────────

export interface ExercisePrescription {
  exerciseId: string;
  sets: number | null;
  reps: number | null;
  durationSeconds: number | null;
  holdSeconds: number | null;
  timesPerDay: number | null;
  daysPerWeek: number | null;
  resistance: string | null;
  painLimit: number | null;
  notes: string | null;
}

// ─── Program ─────────────────────────────────────────────────────────────────

export interface HEPProgram {
  programId: string;
  patientId: string;
  encounterId: string | null;
  createdBy: string;
  exercises: ExercisePrescription[];
  notes: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface CreateHepProgramInput {
  patientId: string;
  encounterId?: string | null;
  exercises: ExercisePrescription[];
  notes?: string | null;
}

// ─── Template ────────────────────────────────────────────────────────────────

export interface HEPTemplate {
  templateId: string;
  name: string;
  bodyRegion: ExerciseRegion | null;
  conditionName: string | null;
  exercises: ExercisePrescription[];
  createdBy: string;
  isBuiltin: boolean;
  createdAt: string;
}

export interface CreateHepTemplateInput {
  name: string;
  bodyRegion?: ExerciseRegion | null;
  conditionName?: string | null;
  exercises: ExercisePrescription[];
}
